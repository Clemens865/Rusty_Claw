//! Gateway method handlers.

use std::sync::Arc;

use serde_json::json;
use tracing::{debug, info};

use rusty_claw_core::protocol::{ErrorShape, GatewayFrame};
use rusty_claw_core::session::{Session, SessionKey};
use rusty_claw_core::types::{ChatType, InboundMessage};
use rusty_claw_agent::AgentEvent;

use crate::events::broadcast_event;
use crate::state::GatewayState;

/// Dispatch a method request and return the response frame.
pub async fn dispatch_method(
    state: &Arc<GatewayState>,
    request_id: &str,
    method: &str,
    params: Option<serde_json::Value>,
) -> GatewayFrame {
    debug!(method, "Dispatching method");

    match method {
        "sessions.list" => handle_sessions_list(state, request_id).await,
        "sessions.preview" => handle_sessions_preview(state, request_id, params).await,
        "sessions.delete" => handle_sessions_delete(state, request_id, params).await,
        "sessions.reset" => handle_sessions_reset(state, request_id, params).await,
        "sessions.patch" => handle_sessions_patch(state, request_id, params).await,
        "agent" => handle_agent(state, request_id, params).await,
        "wake" => ok_response(request_id, json!({"status": "ok"})),
        "models.list" => handle_models_list(state, request_id).await,
        "channels.status" => handle_channels_status(state, request_id).await,
        "config.get" => handle_config_get(state, request_id, params).await,
        "config.set" => handle_config_set(state, request_id, params).await,
        "skills.list" => handle_skills_list(state, request_id).await,
        "skills.get" => handle_skills_get(state, request_id, params).await,
        "node.pair.request" => {
            crate::nodes::handle_pair_request(&state.pairing, request_id, params)
        }
        "node.pair.approve" => {
            crate::nodes::handle_pair_approve(&state.pairing, request_id, params)
        }
        "node.invoke" => crate::nodes::handle_invoke(request_id, params),
        "node.event" => crate::nodes::handle_event(request_id, params),
        _ => error_response(
            request_id,
            "method_not_found",
            &format!("Unknown method: {method}"),
        ),
    }
}

async fn handle_sessions_list(state: &Arc<GatewayState>, request_id: &str) -> GatewayFrame {
    match state.sessions.list().await {
        Ok(metas) => {
            let sessions: Vec<serde_json::Value> = metas
                .iter()
                .map(|m| {
                    json!({
                        "key": m.key,
                        "label": m.label,
                        "model": m.model,
                        "last_updated_at": m.last_updated_at.to_rfc3339(),
                    })
                })
                .collect();
            ok_response(request_id, json!({ "sessions": sessions }))
        }
        Err(e) => error_response(request_id, "session_error", &e.to_string()),
    }
}

async fn handle_sessions_preview(
    state: &Arc<GatewayState>,
    request_id: &str,
    params: Option<serde_json::Value>,
) -> GatewayFrame {
    let params = params.unwrap_or_default();
    let key: SessionKey = match serde_json::from_value(params.get("key").cloned().unwrap_or_default()) {
        Ok(k) => k,
        Err(e) => return error_response(request_id, "invalid_params", &e.to_string()),
    };

    let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;

    match state.sessions.load(&key).await {
        Ok(Some(session)) => {
            let entries: Vec<&_> = session.transcript.iter().rev().take(limit).collect();
            ok_response(request_id, json!({ "entries": entries }))
        }
        Ok(None) => error_response(request_id, "not_found", "Session not found"),
        Err(e) => error_response(request_id, "session_error", &e.to_string()),
    }
}

async fn handle_sessions_delete(
    state: &Arc<GatewayState>,
    request_id: &str,
    params: Option<serde_json::Value>,
) -> GatewayFrame {
    let params = params.unwrap_or_default();
    let key: SessionKey = match serde_json::from_value(params.get("key").cloned().unwrap_or_default()) {
        Ok(k) => k,
        Err(e) => return error_response(request_id, "invalid_params", &e.to_string()),
    };

    match state.sessions.delete(&key).await {
        Ok(()) => {
            state.bump_state_version();
            ok_response(request_id, json!({"deleted": true}))
        }
        Err(e) => error_response(request_id, "session_error", &e.to_string()),
    }
}

async fn handle_sessions_reset(
    state: &Arc<GatewayState>,
    request_id: &str,
    params: Option<serde_json::Value>,
) -> GatewayFrame {
    let params = params.unwrap_or_default();
    let key: SessionKey = match serde_json::from_value(params.get("key").cloned().unwrap_or_default()) {
        Ok(k) => k,
        Err(e) => return error_response(request_id, "invalid_params", &e.to_string()),
    };

    match state.sessions.reset(&key).await {
        Ok(()) => {
            state.bump_state_version();
            ok_response(request_id, json!({"reset": true}))
        }
        Err(e) => error_response(request_id, "session_error", &e.to_string()),
    }
}

async fn handle_agent(
    state: &Arc<GatewayState>,
    request_id: &str,
    params: Option<serde_json::Value>,
) -> GatewayFrame {
    let params = params.unwrap_or_default();
    let text = params
        .get("text")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let message = InboundMessage::from_cli_text(&text);

    // Resolve session key
    let key = SessionKey {
        channel: "gateway".into(),
        account_id: "ws-client".into(),
        chat_type: ChatType::Dm,
        peer_id: "ws-client".into(),
        scope: rusty_claw_core::session::SessionScope::PerSender,
    };

    let mut session = match state.sessions.load(&key).await {
        Ok(Some(s)) => s,
        Ok(None) => Session::new(key.clone()),
        Err(e) => return error_response(request_id, "session_error", &e.to_string()),
    };

    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel::<AgentEvent>();

    // Spawn event forwarder
    let state_clone = state.clone();
    tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            if let Ok(payload) = serde_json::to_value(&event) {
                broadcast_event(&state_clone, "agent.event", Some(payload)).await;
            }
        }
    });

    let (provider, credentials) = match state.providers.default() {
        Some(pc) => pc,
        None => return error_response(request_id, "no_provider", "No default provider configured"),
    };

    info!("Starting agent run via gateway");
    let result = rusty_claw_agent::run_agent(
        &mut session,
        message,
        &state.config,
        &state.tools,
        provider,
        credentials,
        event_tx,
        &state.hooks,
    )
    .await;

    // Save session
    if let Err(e) = state.sessions.save(&session).await {
        tracing::error!(%e, "Failed to save session");
    }

    match result {
        Ok(run_result) => ok_response(request_id, serde_json::to_value(&run_result).unwrap_or_default()),
        Err(e) => error_response(request_id, "agent_error", &e.to_string()),
    }
}

async fn handle_models_list(state: &Arc<GatewayState>, request_id: &str) -> GatewayFrame {
    let mut all_models = Vec::new();
    for provider_id in state.providers.list_ids() {
        if let Some((provider, credentials)) = state.providers.get(provider_id) {
            match provider.list_models(credentials).await {
                Ok(models) => all_models.extend(models),
                Err(e) => {
                    tracing::warn!(provider = provider_id, %e, "Failed to list models");
                }
            }
        }
    }
    ok_response(request_id, json!({ "models": all_models }))
}

async fn handle_channels_status(state: &Arc<GatewayState>, request_id: &str) -> GatewayFrame {
    let mut statuses = Vec::new();
    for ch_id in state.channels.list() {
        if let Some(ch) = state.channels.get(ch_id) {
            let status = ch.status().await;
            statuses.push(json!({
                "id": ch_id,
                "connected": status.connected,
                "display_name": status.display_name,
                "error": status.error,
            }));
        }
    }
    ok_response(request_id, json!({ "channels": statuses }))
}

async fn handle_sessions_patch(
    state: &Arc<GatewayState>,
    request_id: &str,
    params: Option<serde_json::Value>,
) -> GatewayFrame {
    let params = params.unwrap_or_default();
    let key: SessionKey =
        match serde_json::from_value(params.get("key").cloned().unwrap_or_default()) {
            Ok(k) => k,
            Err(e) => return error_response(request_id, "invalid_params", &e.to_string()),
        };

    match state.sessions.load(&key).await {
        Ok(Some(mut session)) => {
            // Apply patches
            if let Some(label) = params.get("label").and_then(|v| v.as_str()) {
                session.meta.label = Some(label.to_string());
            }
            if let Some(model) = params.get("model").and_then(|v| v.as_str()) {
                session.meta.model = Some(model.to_string());
            }
            if let Some(thinking) = params.get("thinking_level").and_then(|v| v.as_str()) {
                if let Ok(level) = serde_json::from_value(json!(thinking)) {
                    session.meta.thinking_level = level;
                }
            }

            match state.sessions.save(&session).await {
                Ok(()) => {
                    state.bump_state_version();
                    ok_response(request_id, json!({"patched": true}))
                }
                Err(e) => error_response(request_id, "session_error", &e.to_string()),
            }
        }
        Ok(None) => error_response(request_id, "not_found", "Session not found"),
        Err(e) => error_response(request_id, "session_error", &e.to_string()),
    }
}

async fn handle_config_get(
    state: &Arc<GatewayState>,
    request_id: &str,
    params: Option<serde_json::Value>,
) -> GatewayFrame {
    let params = params.unwrap_or_default();
    let path = params
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if path.is_empty() {
        // Return entire config
        match serde_json::to_value(state.config.as_ref()) {
            Ok(v) => ok_response(request_id, v),
            Err(e) => error_response(request_id, "config_error", &e.to_string()),
        }
    } else {
        match state.config.get_path(path) {
            Some(value) => ok_response(request_id, json!({ "path": path, "value": value })),
            None => error_response(
                request_id,
                "not_found",
                &format!("Config path not found: {path}"),
            ),
        }
    }
}

async fn handle_config_set(
    _state: &Arc<GatewayState>,
    request_id: &str,
    params: Option<serde_json::Value>,
) -> GatewayFrame {
    let params = params.unwrap_or_default();
    let _path = params.get("path").and_then(|v| v.as_str()).unwrap_or("");
    let _value = params.get("value");

    // Config is currently Arc<Config> (immutable). Full config.set requires
    // the hot-reload system with Arc<RwLock<Config>>. For now, return a
    // placeholder that acknowledges the intent.
    error_response(
        request_id,
        "not_implemented",
        "config.set requires hot-reload mode. Start gateway with --watch-config to enable.",
    )
}

async fn handle_skills_list(state: &Arc<GatewayState>, request_id: &str) -> GatewayFrame {
    let skills = state.skills.read().await;
    let skill_list: Vec<serde_json::Value> = skills
        .all()
        .iter()
        .map(|s| {
            json!({
                "name": s.name,
                "description": s.description,
                "tags": s.tags,
                "tools": s.tools,
            })
        })
        .collect();
    ok_response(request_id, json!({ "skills": skill_list }))
}

async fn handle_skills_get(
    state: &Arc<GatewayState>,
    request_id: &str,
    params: Option<serde_json::Value>,
) -> GatewayFrame {
    let params = params.unwrap_or_default();
    let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");

    if name.is_empty() {
        return error_response(request_id, "invalid_params", "name is required");
    }

    let skills = state.skills.read().await;
    match skills.get(name) {
        Some(skill) => {
            match serde_json::to_value(skill) {
                Ok(v) => ok_response(request_id, v),
                Err(e) => error_response(request_id, "serialization_error", &e.to_string()),
            }
        }
        None => error_response(request_id, "not_found", &format!("Skill not found: {name}")),
    }
}

fn ok_response(id: &str, payload: serde_json::Value) -> GatewayFrame {
    GatewayFrame::Response {
        id: id.to_string(),
        ok: true,
        payload: Some(payload),
        error: None,
    }
}

fn error_response(id: &str, code: &str, message: &str) -> GatewayFrame {
    GatewayFrame::Response {
        id: id.to_string(),
        ok: false,
        payload: None,
        error: Some(ErrorShape {
            code: code.to_string(),
            message: message.to_string(),
            details: None,
        }),
    }
}
