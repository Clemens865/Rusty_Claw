//! WebSocket gateway server implementing OpenClaw protocol v3.
//!
//! The gateway is the central hub: it hosts the WebSocket server, manages
//! session state, dispatches agent runs, broadcasts events to all connected
//! clients, and coordinates tool execution.

pub mod channel_router;
pub mod connection;
pub mod cron;
pub mod events;
pub mod hot_reload;
pub mod methods;
pub mod server;
pub mod state;

pub use cron::CronScheduler;
pub use hot_reload::ConfigWatcher;
pub use server::start_gateway;
pub use state::GatewayState;
