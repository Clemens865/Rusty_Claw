//! Gateway method handlers.

use std::sync::Arc;

use serde_json::json;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use rusty_claw_core::config::CronJob;
use rusty_claw_core::protocol::{ErrorShape, GatewayFrame};
use rusty_claw_core::session::{Session, SessionKey};
use rusty_claw_core::types::{ChatType, InboundMessage};
use rusty_claw_agent::AgentEvent;
use rusty_claw_media::voice_session::{TalkMode, VoiceSession};

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

    #[cfg(feature = "metrics")]
    let start = std::time::Instant::now();

    let response = dispatch_method_inner(state, request_id, method, params).await;

    #[cfg(feature = "metrics")]
    crate::metrics::record_request(method, start.elapsed().as_secs_f64());

    response
}

async fn dispatch_method_inner(
    state: &Arc<GatewayState>,
    request_id: &str,
    method: &str,
    params: Option<serde_json::Value>,
) -> GatewayFrame {
    match method {
        "sessions.list" => handle_sessions_list(state, request_id).await,
        "sessions.preview" => handle_sessions_preview(state, request_id, params).await,
        "sessions.delete" => handle_sessions_delete(state, request_id, params).await,
        "sessions.reset" => handle_sessions_reset(state, request_id, params).await,
        "sessions.patch" => handle_sessions_patch(state, request_id, params).await,
        "agent" => handle_agent(state, request_id, params).await,
        "agent.abort" => handle_agent_abort(state, request_id, params).await,
        "agent.status" => handle_agent_status(state, request_id, params).await,
        "wake" => ok_response(request_id, json!({"status": "ok"})),
        "models.list" => handle_models_list(state, request_id).await,
        "channels.status" => handle_channels_status(state, request_id).await,
        "channels.login" => handle_channels_login(state, request_id, params).await,
        "channels.logout" => handle_channels_logout(state, request_id, params).await,
        "config.get" => handle_config_get(state, request_id, params).await,
        "config.set" => handle_config_set(state, request_id, params).await,
        "cron.list" => handle_cron_list(state, request_id).await,
        "cron.add" => handle_cron_add(state, request_id, params).await,
        "cron.remove" => handle_cron_remove(state, request_id, params).await,
        "skills.list" => handle_skills_list(state, request_id).await,
        "skills.get" => handle_skills_get(state, request_id, params).await,
        "sessions.compact" => handle_sessions_compact(state, request_id, params).await,
        "talk.config" => handle_talk_config(state, request_id, params).await,
        "talk.start" => handle_talk_start(state, request_id, params).await,
        "talk.stop" => handle_talk_stop(state, request_id, params).await,
        "talk.mode" => handle_talk_mode(state, request_id, params).await,
        "node.pair.request" => {
            crate::nodes::handle_pair_request(&state.pairing, request_id, params)
        }
        "node.pair.approve" => {
            crate::nodes::handle_pair_approve(&state.pairing, request_id, params)
        }
        "agents.spawn" => handle_agents_spawn(state, request_id, params).await,
        "node.invoke" => crate::nodes::handle_invoke(request_id, params),
        "node.event" => crate::nodes::handle_event(request_id, params),
        _ => error_response(
            request_id,
            "method_not_found",
            &format!("Unknown method: {method}"),
        ),
    }
}

// ============================================================
// Session methods
// ============================================================

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
            if let Some(prompt) = params.get("custom_system_prompt") {
                if prompt.is_null() {
                    session.meta.custom_system_prompt = None;
                } else if let Some(s) = prompt.as_str() {
                    session.meta.custom_system_prompt = Some(s.to_string());
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

async fn handle_sessions_compact(
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

    let mut session = match state.sessions.load(&key).await {
        Ok(Some(s)) => s,
        Ok(None) => return error_response(request_id, "not_found", "Session not found"),
        Err(e) => return error_response(request_id, "session_error", &e.to_string()),
    };

    let (provider, credentials) = match state.providers.default() {
        Some(pc) => pc,
        None => {
            return error_response(request_id, "no_provider", "No default provider configured")
        }
    };

    let config = Arc::new(state.read_config().await);

    match rusty_claw_agent::compaction::compact_transcript(
        &mut session,
        &config,
        provider,
        credentials,
        &state.hooks,
    )
    .await
    {
        Ok(true) => {
            if let Err(e) = state.sessions.save(&session).await {
                return error_response(request_id, "session_error", &e.to_string());
            }
            state.bump_state_version();
            let new_tokens =
                rusty_claw_agent::transcript::estimate_transcript_tokens(&session.transcript);
            ok_response(
                request_id,
                json!({"compacted": true, "new_token_estimate": new_tokens}),
            )
        }
        Ok(false) => ok_response(
            request_id,
            json!({"compacted": false, "reason": "under_limit"}),
        ),
        Err(e) => error_response(request_id, "compaction_error", &e.to_string()),
    }
}

// ============================================================
// Agent methods
// ============================================================

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

    let key = SessionKey {
        channel: "gateway".into(),
        account_id: "ws-client".into(),
        chat_type: ChatType::Dm,
        peer_id: "ws-client".into(),
        scope: rusty_claw_core::session::SessionScope::PerSender,
    };

    let session_hash = key.hash_key();

    let mut session = match state.sessions.load(&key).await {
        Ok(Some(s)) => s,
        Ok(None) => Session::new(key.clone()),
        Err(e) => return error_response(request_id, "session_error", &e.to_string()),
    };

    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel::<AgentEvent>();

    // Create cancellation token for this agent run
    let cancel_token = CancellationToken::new();
    {
        let mut active = state.active_agents.write().await;
        active.insert(session_hash.clone(), cancel_token.clone());
    }

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

    // Read config snapshot
    let config = Arc::new(state.read_config().await);

    info!("Starting agent run via gateway");
    let result = rusty_claw_agent::run_agent(
        &mut session,
        message,
        &config,
        &state.tools,
        provider,
        credentials,
        event_tx,
        &state.hooks,
    )
    .await;

    // Remove from active agents
    {
        let mut active = state.active_agents.write().await;
        active.remove(&session_hash);
    }

    // Save session
    if let Err(e) = state.sessions.save(&session).await {
        tracing::error!(%e, "Failed to save session");
    }

    match result {
        Ok(run_result) => ok_response(request_id, serde_json::to_value(&run_result).unwrap_or_default()),
        Err(e) => error_response(request_id, "agent_error", &e.to_string()),
    }
}

async fn handle_agent_abort(
    state: &Arc<GatewayState>,
    request_id: &str,
    params: Option<serde_json::Value>,
) -> GatewayFrame {
    let params = params.unwrap_or_default();
    let session_key = params
        .get("session_key")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if session_key.is_empty() {
        let active = state.active_agents.read().await;
        let count = active.len();
        for token in active.values() {
            token.cancel();
        }
        return ok_response(request_id, json!({"aborted": count}));
    }

    let active = state.active_agents.read().await;
    if let Some(token) = active.get(session_key) {
        token.cancel();
        ok_response(request_id, json!({"aborted": true, "session_key": session_key}))
    } else {
        error_response(
            request_id,
            "not_found",
            &format!("No active agent for session: {session_key}"),
        )
    }
}

async fn handle_agent_status(
    state: &Arc<GatewayState>,
    request_id: &str,
    params: Option<serde_json::Value>,
) -> GatewayFrame {
    let params = params.unwrap_or_default();
    let session_key = params
        .get("session_key")
        .and_then(|v| v.as_str());

    let active = state.active_agents.read().await;

    if let Some(key) = session_key {
        let running = active.contains_key(key);
        ok_response(request_id, json!({"session_key": key, "running": running}))
    } else {
        let running: Vec<&String> = active.keys().collect();
        ok_response(request_id, json!({"active_agents": running}))
    }
}

// ============================================================
// Model + Channel methods
// ============================================================

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

async fn handle_channels_login(
    state: &Arc<GatewayState>,
    request_id: &str,
    params: Option<serde_json::Value>,
) -> GatewayFrame {
    let params = params.unwrap_or_default();
    let channel_id = params
        .get("channel")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if channel_id.is_empty() {
        return error_response(request_id, "invalid_params", "channel is required");
    }

    match state.channels.get(channel_id) {
        Some(ch) => {
            let config_value = json!({});
            match ch.start(&config_value).await {
                Ok((_rx, _handle)) => {
                    info!(channel = channel_id, "Channel logged in via WS method");
                    ok_response(request_id, json!({"channel": channel_id, "logged_in": true}))
                }
                Err(e) => error_response(request_id, "channel_error", &e.to_string()),
            }
        }
        None => error_response(
            request_id,
            "not_found",
            &format!("Channel not found: {channel_id}"),
        ),
    }
}

async fn handle_channels_logout(
    state: &Arc<GatewayState>,
    request_id: &str,
    params: Option<serde_json::Value>,
) -> GatewayFrame {
    let params = params.unwrap_or_default();
    let channel_id = params
        .get("channel")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if channel_id.is_empty() {
        return error_response(request_id, "invalid_params", "channel is required");
    }

    match state.channels.get(channel_id) {
        Some(_ch) => {
            info!(channel = channel_id, "Channel logout requested via WS method");
            ok_response(request_id, json!({"channel": channel_id, "logged_out": true}))
        }
        None => error_response(
            request_id,
            "not_found",
            &format!("Channel not found: {channel_id}"),
        ),
    }
}

// ============================================================
// Config methods
// ============================================================

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

    let config = state.read_config().await;

    if path.is_empty() {
        match serde_json::to_value(&config) {
            Ok(v) => ok_response(request_id, v),
            Err(e) => error_response(request_id, "config_error", &e.to_string()),
        }
    } else {
        match config.get_path(path) {
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
    state: &Arc<GatewayState>,
    request_id: &str,
    params: Option<serde_json::Value>,
) -> GatewayFrame {
    let params = params.unwrap_or_default();
    let path = match params.get("path").and_then(|v| v.as_str()) {
        Some(p) if !p.is_empty() => p.to_string(),
        _ => return error_response(request_id, "invalid_params", "path is required"),
    };
    let value = match params.get("value") {
        Some(v) => v.clone(),
        None => return error_response(request_id, "invalid_params", "value is required"),
    };

    {
        let mut config = state.config.write().await;
        if let Err(e) = config.set_path(&path, value.clone()) {
            return error_response(request_id, "config_error", &e.to_string());
        }

        if let Some(ref config_path) = state.config_path {
            if let Err(e) = config.save(config_path) {
                warn!(%e, "Failed to persist config to disk");
            }
        }
    }

    broadcast_event(
        state,
        "config.changed",
        Some(json!({"path": path, "value": value})),
    )
    .await;

    state.bump_state_version();

    ok_response(request_id, json!({"path": path, "updated": true}))
}

// ============================================================
// Cron methods
// ============================================================

async fn handle_cron_list(state: &Arc<GatewayState>, request_id: &str) -> GatewayFrame {
    match &state.cron {
        Some(scheduler) => {
            let jobs = scheduler.list_jobs().await;
            let job_list: Vec<serde_json::Value> = jobs
                .iter()
                .map(|j| {
                    json!({
                        "id": j.id,
                        "schedule": j.schedule,
                        "task": j.task,
                        "enabled": j.enabled,
                    })
                })
                .collect();
            ok_response(request_id, json!({ "jobs": job_list }))
        }
        None => ok_response(request_id, json!({ "jobs": [] })),
    }
}

async fn handle_cron_add(
    state: &Arc<GatewayState>,
    request_id: &str,
    params: Option<serde_json::Value>,
) -> GatewayFrame {
    let params = params.unwrap_or_default();
    let id = params.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let schedule = params.get("schedule").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let task = params.get("task").and_then(|v| v.as_str()).unwrap_or("").to_string();

    if id.is_empty() || schedule.is_empty() || task.is_empty() {
        return error_response(request_id, "invalid_params", "id, schedule, and task are required");
    }

    let job = CronJob {
        id: id.clone(),
        schedule,
        task,
        session_key: params.get("session_key").and_then(|v| v.as_str()).map(String::from),
        enabled: params.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true),
    };

    match &state.cron {
        Some(scheduler) => match scheduler.add_job(job).await {
            Ok(()) => ok_response(request_id, json!({"added": true, "id": id})),
            Err(e) => error_response(request_id, "cron_error", &e),
        },
        None => error_response(request_id, "not_available", "Cron scheduler not running"),
    }
}

async fn handle_cron_remove(
    state: &Arc<GatewayState>,
    request_id: &str,
    params: Option<serde_json::Value>,
) -> GatewayFrame {
    let params = params.unwrap_or_default();
    let id = params.get("id").and_then(|v| v.as_str()).unwrap_or("");

    if id.is_empty() {
        return error_response(request_id, "invalid_params", "id is required");
    }

    match &state.cron {
        Some(scheduler) => {
            let removed = scheduler.remove_job(id).await;
            ok_response(request_id, json!({"removed": removed, "id": id}))
        }
        None => error_response(request_id, "not_available", "Cron scheduler not running"),
    }
}

// ============================================================
// Skills methods
// ============================================================

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
        Some(skill) => match serde_json::to_value(skill) {
            Ok(v) => ok_response(request_id, v),
            Err(e) => error_response(request_id, "serialization_error", &e.to_string()),
        },
        None => error_response(request_id, "not_found", &format!("Skill not found: {name}")),
    }
}

// ============================================================
// Talk config method
// ============================================================

async fn handle_talk_config(
    state: &Arc<GatewayState>,
    request_id: &str,
    params: Option<serde_json::Value>,
) -> GatewayFrame {
    let params = params.unwrap_or_default();
    let action = params.get("action").and_then(|v| v.as_str()).unwrap_or("get");

    let config = state.read_config().await;

    match action {
        "get" => {
            let tts = config
                .tools
                .as_ref()
                .and_then(|t| t.tts.as_ref())
                .map(|t| json!({"provider": t.provider, "voice": t.default_voice, "model": t.default_model}));
            let transcription = config
                .tools
                .as_ref()
                .and_then(|t| t.transcription.as_ref())
                .map(|t| json!({"provider": t.provider, "model": t.model}));
            ok_response(request_id, json!({"tts": tts, "transcription": transcription}))
        }
        _ => error_response(request_id, "invalid_params", "action must be 'get'"),
    }
}

// ============================================================
// Talk methods (voice pipeline)
// ============================================================

async fn handle_talk_start(
    state: &Arc<GatewayState>,
    request_id: &str,
    params: Option<serde_json::Value>,
) -> GatewayFrame {
    let params = params.unwrap_or_default();
    let conn_id = params
        .get("conn_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if conn_id.is_empty() {
        return error_response(request_id, "invalid_params", "conn_id is required");
    }

    let mode_str = params
        .get("mode")
        .and_then(|v| v.as_str())
        .unwrap_or("vad");

    let mode = match mode_str {
        "push" => TalkMode::Push,
        _ => TalkMode::Vad,
    };

    // Check if there's already an active voice session
    {
        let connections = state.connections.read().await;
        if let Some(conn) = connections.get(conn_id) {
            if conn.voice_session.is_some() {
                return error_response(
                    request_id,
                    "already_active",
                    "Voice session already active",
                );
            }
        } else {
            return error_response(request_id, "not_found", "Connection not found");
        }
    }

    let (handle, mut utterance_rx) = VoiceSession::start(mode);

    // Store voice session handle in connection state
    {
        let mut connections = state.connections.write().await;
        if let Some(conn) = connections.get_mut(conn_id) {
            conn.voice_session = Some(handle);
        }
    }

    // Spawn utterance processing task
    let state_clone = state.clone();
    let conn_id_owned = conn_id.to_string();
    tokio::spawn(async move {
        while let Some(utterance) = utterance_rx.recv().await {
            debug!(
                duration_ms = utterance.duration_ms,
                samples = utterance.pcm_data.len(),
                "Utterance received"
            );

            // Get transcription config
            let config = state_clone.read_config().await;
            let transcription_config = config
                .tools
                .as_ref()
                .and_then(|t| t.transcription.as_ref());

            if let Some(tc) = transcription_config {
                match rusty_claw_media::stt::transcribe_audio_bytes(&utterance.pcm_data, tc).await {
                    Ok(text) if !text.is_empty() => {
                        info!(text = %text, "Transcribed utterance");

                        // Send transcription as agent event
                        let event = AgentEvent::BlockReply {
                            text: format!("[Voice] {text}"),
                            is_final: true,
                        };
                        if let Ok(payload) = serde_json::to_value(&event) {
                            broadcast_event(&state_clone, "agent.event", Some(payload)).await;
                        }
                    }
                    Ok(_) => {
                        debug!("Empty transcription result");
                    }
                    Err(e) => {
                        warn!(%e, "Transcription failed");
                    }
                }
            } else {
                warn!("No transcription config, cannot process voice");
            }
        }

        // Cleanup voice session on task end
        let mut connections = state_clone.connections.write().await;
        if let Some(conn) = connections.get_mut(&conn_id_owned) {
            conn.voice_session = None;
        }
    });

    ok_response(
        request_id,
        json!({"started": true, "mode": mode_str, "conn_id": conn_id}),
    )
}

async fn handle_talk_stop(
    state: &Arc<GatewayState>,
    request_id: &str,
    params: Option<serde_json::Value>,
) -> GatewayFrame {
    let params = params.unwrap_or_default();
    let conn_id = params
        .get("conn_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if conn_id.is_empty() {
        return error_response(request_id, "invalid_params", "conn_id is required");
    }

    let mut connections = state.connections.write().await;
    if let Some(conn) = connections.get_mut(conn_id) {
        if let Some(handle) = conn.voice_session.take() {
            handle.cancel.cancel();
            ok_response(request_id, json!({"stopped": true, "conn_id": conn_id}))
        } else {
            error_response(request_id, "not_active", "No voice session active")
        }
    } else {
        error_response(request_id, "not_found", "Connection not found")
    }
}

async fn handle_talk_mode(
    state: &Arc<GatewayState>,
    request_id: &str,
    params: Option<serde_json::Value>,
) -> GatewayFrame {
    let params = params.unwrap_or_default();
    let conn_id = params
        .get("conn_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let mode_str = params
        .get("mode")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if conn_id.is_empty() || mode_str.is_empty() {
        return error_response(request_id, "invalid_params", "conn_id and mode are required");
    }

    let mode = match mode_str {
        "push" => TalkMode::Push,
        "vad" => TalkMode::Vad,
        _ => return error_response(request_id, "invalid_params", "mode must be 'push' or 'vad'"),
    };

    let mut connections = state.connections.write().await;
    if let Some(conn) = connections.get_mut(conn_id) {
        if let Some(ref mut handle) = conn.voice_session {
            handle.mode = mode;
            ok_response(request_id, json!({"mode": mode_str, "conn_id": conn_id}))
        } else {
            error_response(request_id, "not_active", "No voice session active")
        }
    } else {
        error_response(request_id, "not_found", "Connection not found")
    }
}

// ============================================================
// Agent spawning
// ============================================================

async fn handle_agents_spawn(
    state: &Arc<GatewayState>,
    request_id: &str,
    params: Option<serde_json::Value>,
) -> GatewayFrame {
    let params = params.unwrap_or_default();
    let task = match params.get("task").and_then(|v| v.as_str()) {
        Some(t) if !t.is_empty() => t.to_string(),
        _ => return error_response(request_id, "invalid_params", "task is required"),
    };
    let model = params.get("model").and_then(|v| v.as_str()).map(String::from);

    // Check spawn depth
    let parent_depth = params
        .get("spawn_depth")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;

    let config = state.read_config().await;
    let max_depth = config.max_spawn_depth();

    if parent_depth >= max_depth {
        return error_response(
            request_id,
            "spawn_depth_exceeded",
            &format!("Max spawn depth of {max_depth} exceeded"),
        );
    }

    // Create child session
    let child_key = SessionKey {
        channel: "spawned".into(),
        account_id: "agent".into(),
        chat_type: ChatType::Dm,
        peer_id: format!("spawn-{}", uuid::Uuid::new_v4()),
        scope: rusty_claw_core::session::SessionScope::PerSender,
    };
    let child_hash = child_key.hash_key();

    let mut child_session = Session::new(child_key);
    child_session.meta.spawned_by = Some("parent".to_string());
    child_session.meta.spawn_depth = parent_depth + 1;
    if let Some(ref m) = model {
        child_session.meta.model = Some(m.clone());
    }

    // Save child session
    if let Err(e) = state.sessions.save(&child_session).await {
        return error_response(request_id, "session_error", &e.to_string());
    }

    // Spawn agent task for the child
    let state_clone = state.clone();
    let task_clone = task.clone();
    tokio::spawn(async move {
        let message = InboundMessage::from_cli_text(&task_clone);
        let (event_tx, _event_rx) = tokio::sync::mpsc::unbounded_channel();

        let (provider, credentials) = match state_clone.providers.default() {
            Some(pc) => pc,
            None => {
                warn!("No provider for spawned agent");
                return;
            }
        };

        let config = Arc::new(state_clone.read_config().await);
        let _ = rusty_claw_agent::run_agent(
            &mut child_session,
            message,
            &config,
            &state_clone.tools,
            provider,
            credentials,
            event_tx,
            &state_clone.hooks,
        )
        .await;

        if let Err(e) = state_clone.sessions.save(&child_session).await {
            warn!(%e, "Failed to save spawned session");
        }
    });

    info!(child = %child_hash, "Spawned child agent");
    ok_response(
        request_id,
        json!({"spawned": true, "child_session_key": child_hash}),
    )
}

// ============================================================
// Helpers
// ============================================================

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
