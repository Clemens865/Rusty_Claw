//! Gateway integration tests — start a real gateway and interact via WS + HTTP.
//!
//! Run with: `cargo test -p rusty-claw-gateway --test integration`

use std::sync::Arc;

use futures::{SinkExt, StreamExt};
use serde_json::json;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

/// Find an available port.
fn find_free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

/// Build a minimal gateway and return its state + port.
async fn start_test_gateway() -> (Arc<rusty_claw_gateway::GatewayState>, u16) {
    let port = find_free_port();

    let config = rusty_claw_core::config::Config::default();
    let config_rw = Arc::new(tokio::sync::RwLock::new(config));

    let sessions: Arc<dyn rusty_claw_core::session::SessionStore> = Arc::new(
        rusty_claw_core::session_store::JsonlSessionStore::new(
            std::env::temp_dir().join(format!("rusty-claw-test-{port}")),
        ),
    );

    let channels = Arc::new(rusty_claw_channels::ChannelRegistry::new());
    let mut tools = rusty_claw_tools::ToolRegistry::new();
    rusty_claw_tools::register_builtin_tools(&mut tools);
    let tools = Arc::new(tools);

    let providers = Arc::new(rusty_claw_providers::ProviderRegistry::new(
        "none".into(),
    ));

    let hooks = Arc::new(rusty_claw_plugins::HookRegistry::new());
    let skills = rusty_claw_gateway::skills::SkillRegistry::new();
    let pairing = rusty_claw_core::pairing::PairingStore::new(
        std::env::temp_dir().join(format!("rusty-claw-pairing-{port}")),
    );

    let state = Arc::new(rusty_claw_gateway::GatewayState::new(
        config_rw,
        None,
        sessions,
        channels,
        tools,
        providers,
        hooks,
        skills,
        pairing,
        None,
        None,
    ));

    // Start gateway in background
    let state_clone = state.clone();
    tokio::spawn(async move {
        let _ = rusty_claw_gateway::start_gateway(state_clone, port, false).await;
    });

    // Wait for gateway to be ready
    for _ in 0..50 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        if reqwest::get(format!("http://127.0.0.1:{port}/health"))
            .await
            .is_ok()
        {
            break;
        }
    }

    (state, port)
}

#[tokio::test]
async fn test_health_endpoint() {
    let (_state, port) = start_test_gateway().await;

    let resp = reqwest::get(format!("http://127.0.0.1:{port}/health"))
        .await
        .expect("Health request failed");

    assert!(resp.status().is_success());
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "ok");
    assert!(body["version"].is_string());
}

#[tokio::test]
async fn test_ws_hello_and_sessions_list() {
    let (_state, port) = start_test_gateway().await;

    let url = format!("ws://127.0.0.1:{port}/ws");
    let (mut ws, _) = connect_async(&url).await.expect("WS connect failed");

    // Should receive HelloOk event
    let msg = ws.next().await.unwrap().unwrap();
    let hello: serde_json::Value =
        serde_json::from_str(&msg.to_text().unwrap()).unwrap();
    assert_eq!(hello["event"], "hello");

    let payload = &hello["payload"];
    assert_eq!(payload["protocol"], 3);
    assert!(payload["features"]["methods"].is_array());

    // Send sessions.list request
    let req = json!({
        "type": "req",
        "id": "test-1",
        "method": "sessions.list",
    });
    ws.send(Message::Text(req.to_string().into())).await.unwrap();

    // Read response
    let resp_msg = ws.next().await.unwrap().unwrap();
    let resp: serde_json::Value =
        serde_json::from_str(&resp_msg.to_text().unwrap()).unwrap();
    assert_eq!(resp["id"], "test-1");
    assert_eq!(resp["ok"], true);
    assert!(resp["payload"]["sessions"].is_array());

    ws.close(None).await.ok();
}

#[tokio::test]
async fn test_ws_config_get() {
    let (_state, port) = start_test_gateway().await;

    let url = format!("ws://127.0.0.1:{port}/ws");
    let (mut ws, _) = connect_async(&url).await.expect("WS connect failed");

    // Skip hello
    let _ = ws.next().await;

    // Send config.get request (no path = full config)
    let req = json!({
        "type": "req",
        "id": "cfg-1",
        "method": "config.get",
    });
    ws.send(Message::Text(req.to_string().into())).await.unwrap();

    let resp_msg = ws.next().await.unwrap().unwrap();
    let resp: serde_json::Value =
        serde_json::from_str(&resp_msg.to_text().unwrap()).unwrap();
    assert_eq!(resp["id"], "cfg-1");
    assert_eq!(resp["ok"], true);

    ws.close(None).await.ok();
}

#[tokio::test]
async fn test_ws_config_set_and_get() {
    let (_state, port) = start_test_gateway().await;

    let url = format!("ws://127.0.0.1:{port}/ws");
    let (mut ws, _) = connect_async(&url).await.expect("WS connect failed");

    // Skip hello
    let _ = ws.next().await;

    // Set a config value
    let set_req = json!({
        "type": "req",
        "id": "set-1",
        "method": "config.set",
        "params": {
            "path": "agents.defaults.model",
            "value": "gpt-4o"
        }
    });
    ws.send(Message::Text(set_req.to_string().into())).await.unwrap();

    // Read set response (and possibly a config.changed event)
    let mut set_ok = false;
    for _ in 0..5 {
        let msg = ws.next().await.unwrap().unwrap();
        let resp: serde_json::Value = serde_json::from_str(&msg.to_text().unwrap()).unwrap();
        if resp.get("id").and_then(|v| v.as_str()) == Some("set-1") {
            assert_eq!(resp["ok"], true);
            set_ok = true;
            break;
        }
    }
    assert!(set_ok, "Did not receive config.set response");

    ws.close(None).await.ok();
}

#[tokio::test]
async fn test_ws_unknown_method() {
    let (_state, port) = start_test_gateway().await;

    let url = format!("ws://127.0.0.1:{port}/ws");
    let (mut ws, _) = connect_async(&url).await.expect("WS connect failed");

    // Skip hello
    let _ = ws.next().await;

    // Send unknown method
    let req = json!({
        "type": "req",
        "id": "bad-1",
        "method": "nonexistent.method",
    });
    ws.send(Message::Text(req.to_string().into())).await.unwrap();

    let resp_msg = ws.next().await.unwrap().unwrap();
    let resp: serde_json::Value =
        serde_json::from_str(&resp_msg.to_text().unwrap()).unwrap();
    assert_eq!(resp["id"], "bad-1");
    assert_eq!(resp["ok"], false);
    assert!(resp["error"]["code"]
        .as_str()
        .unwrap()
        .contains("not_found"));

    ws.close(None).await.ok();
}

#[tokio::test]
async fn test_ws_skills_list() {
    let (_state, port) = start_test_gateway().await;

    let url = format!("ws://127.0.0.1:{port}/ws");
    let (mut ws, _) = connect_async(&url).await.expect("WS connect failed");

    // Skip hello
    let _ = ws.next().await;

    let req = json!({
        "type": "req",
        "id": "sk-1",
        "method": "skills.list",
    });
    ws.send(Message::Text(req.to_string().into())).await.unwrap();

    let resp_msg = ws.next().await.unwrap().unwrap();
    let resp: serde_json::Value =
        serde_json::from_str(&resp_msg.to_text().unwrap()).unwrap();
    assert_eq!(resp["id"], "sk-1");
    assert_eq!(resp["ok"], true);
    assert!(resp["payload"]["skills"].is_array());

    ws.close(None).await.ok();
}

#[tokio::test]
async fn test_ws_cron_list() {
    let (_state, port) = start_test_gateway().await;

    let url = format!("ws://127.0.0.1:{port}/ws");
    let (mut ws, _) = connect_async(&url).await.expect("WS connect failed");

    // Skip hello
    let _ = ws.next().await;

    let req = json!({
        "type": "req",
        "id": "cr-1",
        "method": "cron.list",
    });
    ws.send(Message::Text(req.to_string().into())).await.unwrap();

    let resp_msg = ws.next().await.unwrap().unwrap();
    let resp: serde_json::Value =
        serde_json::from_str(&resp_msg.to_text().unwrap()).unwrap();
    assert_eq!(resp["id"], "cr-1");
    assert_eq!(resp["ok"], true);
    assert!(resp["payload"]["jobs"].is_array());

    ws.close(None).await.ok();
}

#[tokio::test]
async fn test_ws_agent_status() {
    let (_state, port) = start_test_gateway().await;

    let url = format!("ws://127.0.0.1:{port}/ws");
    let (mut ws, _) = connect_async(&url).await.expect("WS connect failed");

    // Skip hello
    let _ = ws.next().await;

    let req = json!({
        "type": "req",
        "id": "as-1",
        "method": "agent.status",
    });
    ws.send(Message::Text(req.to_string().into())).await.unwrap();

    let resp_msg = ws.next().await.unwrap().unwrap();
    let resp: serde_json::Value =
        serde_json::from_str(&resp_msg.to_text().unwrap()).unwrap();
    assert_eq!(resp["id"], "as-1");
    assert_eq!(resp["ok"], true);
    assert!(resp["payload"]["active_agents"].is_array());

    ws.close(None).await.ok();
}
