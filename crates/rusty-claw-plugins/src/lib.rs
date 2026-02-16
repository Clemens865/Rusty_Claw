//! Plugin SDK and runtime for extending Rusty Claw.
//!
//! Plugins can register tools, hooks, channels, providers, HTTP routes,
//! gateway methods, and CLI commands.

use async_trait::async_trait;

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
pub struct PluginApi {
    // TODO: Registration methods for:
    // - Tools (Box<dyn Tool>)
    // - Hooks (HookEvent -> handler)
    // - Channels (Box<dyn Channel>)
    // - Providers (Box<dyn LlmProvider>)
    // - HTTP routes
    // - Gateway methods
    // - CLI commands
    // - Chat commands
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
