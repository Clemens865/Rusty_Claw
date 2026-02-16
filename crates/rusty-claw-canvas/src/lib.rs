//! Canvas/A2UI host — agent-driven visual workspace.
//!
//! The canvas system allows agents to push HTML/JS to connected browser clients
//! for real-time visual interaction (A2UI pattern).

pub mod protocol;
pub mod session;

pub use protocol::{CanvasEvent, CanvasOperation};
pub use session::CanvasSession;
