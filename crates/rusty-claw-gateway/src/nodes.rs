//! Node protocol — device pairing and remote invocation.

use serde_json::json;

use rusty_claw_core::pairing::PairingStore;
use rusty_claw_core::protocol::{ErrorShape, GatewayFrame};

/// Handle `node.pair.request` — initiate a pairing request from a device.
pub fn handle_pair_request(
    pairing: &PairingStore,
    request_id: &str,
    params: Option<serde_json::Value>,
) -> GatewayFrame {
    let params = params.unwrap_or_default();
    let channel = params.get("channel").and_then(|v| v.as_str()).unwrap_or("gateway");
    let sender_id = params.get("sender_id").and_then(|v| v.as_str()).unwrap_or("");
    let display_name = params.get("display_name").and_then(|v| v.as_str()).map(String::from);

    if sender_id.is_empty() {
        return error_response(request_id, "invalid_params", "sender_id is required");
    }

    // Check if already approved
    if pairing.is_approved(channel, sender_id) {
        return ok_response(request_id, json!({"status": "already_approved"}));
    }

    match pairing.create_request(channel, sender_id, display_name) {
        Ok(code) => ok_response(request_id, json!({
            "status": "pending",
            "code": code,
            "message": "Pairing request created. Approve via CLI or gateway."
        })),
        Err(e) => error_response(request_id, "pairing_error", &e.to_string()),
    }
}

/// Handle `node.pair.approve` — approve a pending pairing request.
pub fn handle_pair_approve(
    pairing: &PairingStore,
    request_id: &str,
    params: Option<serde_json::Value>,
) -> GatewayFrame {
    let params = params.unwrap_or_default();
    let channel = params.get("channel").and_then(|v| v.as_str()).unwrap_or("gateway");
    let code = params.get("code").and_then(|v| v.as_str()).unwrap_or("");

    if code.is_empty() {
        return error_response(request_id, "invalid_params", "code is required");
    }

    match pairing.approve(channel, code) {
        Ok(true) => ok_response(request_id, json!({"approved": true})),
        Ok(false) => error_response(request_id, "not_found", "No pending pairing with that code"),
        Err(e) => error_response(request_id, "pairing_error", &e.to_string()),
    }
}

/// Handle `node.invoke` — remote command invocation (placeholder).
pub fn handle_invoke(
    request_id: &str,
    _params: Option<serde_json::Value>,
) -> GatewayFrame {
    // TODO: Route invoke to connected node sessions
    error_response(
        request_id,
        "not_implemented",
        "Remote node invocation not yet implemented",
    )
}

/// Handle `node.event` — node event broadcast (placeholder).
pub fn handle_event(
    request_id: &str,
    _params: Option<serde_json::Value>,
) -> GatewayFrame {
    // TODO: Broadcast events to node sessions
    ok_response(request_id, json!({"status": "accepted"}))
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
