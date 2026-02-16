//! WebSocket gateway server implementing OpenClaw protocol v3.
//!
//! The gateway is the central hub: it hosts the WebSocket server, manages
//! session state, dispatches agent runs, broadcasts events to all connected
//! clients, and coordinates tool execution.

// TODO: Implement gateway modules:
// pub mod server;       // WebSocket server (axum + tungstenite)
// pub mod connection;   // Connection lifecycle, handshake, auth
// pub mod methods;      // Gateway method handlers (sessions, agent, config, ...)
// pub mod events;       // Event broadcasting
// pub mod state;        // Gateway shared state (sessions, presence, health)
