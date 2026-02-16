//! OpenClaw gateway wire protocol v3.
//!
//! All gateway communication uses JSON-over-WebSocket with three frame types:
//! Request, Response, and Event.

use serde::{Deserialize, Serialize};

/// Protocol version implemented by this gateway.
pub const PROTOCOL_VERSION: u32 = 3;

/// A gateway wire frame — the top-level message envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum GatewayFrame {
    /// Client -> Server request.
    #[serde(rename = "req")]
    Request {
        id: String,
        method: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        params: Option<serde_json::Value>,
    },

    /// Server -> Client response.
    #[serde(rename = "res")]
    Response {
        id: String,
        ok: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        payload: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<ErrorShape>,
    },

    /// Server -> Client event broadcast.
    #[serde(rename = "event")]
    Event {
        event: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        payload: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        seq: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        state_version: Option<StateVersion>,
    },
}

/// Error shape returned in response frames.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorShape {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

/// Monotonic state version counters for client staleness detection.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StateVersion {
    pub presence: u64,
    pub health: u64,
}

/// Client handshake parameters (sent after `connect.challenge`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectParams {
    pub min_protocol: u32,
    pub max_protocol: u32,
    pub client: ClientInfo,
    #[serde(default)]
    pub caps: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<AuthParams>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<DeviceParams>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientInfo {
    pub id: String,
    pub display_name: Option<String>,
    pub version: Option<String>,
    pub platform: Option<String>,
    pub device_family: Option<String>,
    pub mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AuthParams {
    #[serde(rename = "token")]
    Token { token: String },
    #[serde(rename = "password")]
    Password { password: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceParams {
    pub public_key: String,
    pub signature: String,
}

/// Server hello response after successful handshake.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelloOk {
    pub protocol: u32,
    pub server: ServerInfo,
    pub features: Features,
    pub snapshot: Snapshot,
    pub policy: Policy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    pub version: String,
    pub commit: Option<String>,
    pub conn_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Features {
    pub methods: Vec<String>,
    pub events: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub state_version: StateVersion,
    pub auth_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Policy {
    pub max_payload: usize,
    pub max_buffered_bytes: usize,
    pub tick_interval_ms: u64,
}
