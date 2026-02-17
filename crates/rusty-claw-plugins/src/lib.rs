//! Plugin SDK and runtime for extending Rusty Claw.
//!
//! Plugins can register tools, hooks, channels, providers, HTTP routes,
//! gateway methods, and CLI commands.

use async_trait::async_trait;

pub mod api;
pub mod hooks;
pub mod logging_plugin;
pub mod manager;
#[cfg(feature = "wasm")]
pub mod wasm_adapter;
#[cfg(feature = "wasm")]
pub mod wasm_runtime;

pub use hooks::{HookContext, HookHandler, HookRegistry, HookResult};
pub use manager::{PluginManager, PluginRegistrations};

/// Lifecycle hook events that plugins can subscribe to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HookEvent {
    BeforeAgentStart,
    LlmInput,
    LlmOutput,
    AgentEnd,
    BeforeCompaction,
    AfterCompaction,
    BeforeReset,
    MessageReceived,
    MessageSending,
    MessageSent,
    BeforeToolCall,
    AfterToolCall,
    ToolResultPersist,
    SessionStart,
    SessionEnd,
    GatewayStart,
    GatewayStop,
}

/// Plugin registration API.
///
/// Passed to plugins during registration so they can register their
/// extensions with the runtime.
pub use api::PluginApi;

/// Where a plugin was loaded from.
#[derive(Debug, Clone)]
pub enum PluginSource {
    /// Built-in native Rust plugin.
    Native,
    /// Loaded from a WASM file.
    #[cfg(feature = "wasm")]
    Wasm {
        path: std::path::PathBuf,
    },
}

/// The core plugin trait.
#[async_trait]
pub trait Plugin: Send + Sync + 'static {
    /// Plugin identifier.
    fn id(&self) -> &str;

    /// Human-readable plugin name.
    fn name(&self) -> &str;

    /// Register extensions with the runtime.
    fn register(&self, api: &mut PluginApi);
}
