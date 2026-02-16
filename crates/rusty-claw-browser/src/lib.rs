//! CDP browser automation.
//!
//! Provides browser pool management and page automation tools.
//! Requires the `browser` feature flag and Chrome/Chromium installed.

pub mod config;
pub mod pool;

pub use pool::BrowserPool;
