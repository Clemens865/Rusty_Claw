//! Plugin registration API.
//!
//! [`PluginApi`] is passed to each plugin during initialization so it can
//! register tools, hooks, and other extensions with the runtime.

use crate::hooks::HookHandler;
use crate::HookEvent;
use rusty_claw_tools::Tool;

/// Registration API handed to plugins during [`Plugin::register`].
///
/// Collected hooks are applied to the [`HookRegistry`] by the
/// [`PluginManager`] after `register` returns.
pub struct PluginApi {
    tools: Vec<Box<dyn Tool>>,
    pending_hooks: Vec<(HookEvent, HookHandler)>,
}

impl PluginApi {
    pub(crate) fn new() -> Self {
        Self {
            tools: Vec::new(),
            pending_hooks: Vec::new(),
        }
    }

    /// Register a tool that will be added to the agent's tool registry.
    pub fn register_tool(&mut self, tool: Box<dyn Tool>) {
        self.tools.push(tool);
    }

    /// Queue a hook handler for registration.
    ///
    /// The hook will be registered with the [`HookRegistry`] after
    /// `Plugin::register` completes.
    pub fn register_hook(&mut self, event: HookEvent, handler: HookHandler) {
        self.pending_hooks.push((event, handler));
    }

    /// Take all registered tools out of this API (consuming them).
    pub(crate) fn take_tools(&mut self) -> Vec<Box<dyn Tool>> {
        std::mem::take(&mut self.tools)
    }

    /// Take all pending hooks out of this API (consuming them).
    pub(crate) fn take_hooks(&mut self) -> Vec<(HookEvent, HookHandler)> {
        std::mem::take(&mut self.pending_hooks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hooks::{HookRegistry, HookResult};
    use async_trait::async_trait;
    use rusty_claw_tools::{ToolContext, ToolOutput};
    use std::sync::Arc;

    struct DummyTool;

    #[async_trait]
    impl Tool for DummyTool {
        fn name(&self) -> &str {
            "dummy"
        }
        fn description(&self) -> &str {
            "A test tool"
        }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({"type": "object", "properties": {}})
        }
        async fn execute(
            &self,
            _params: serde_json::Value,
            _context: &ToolContext,
        ) -> anyhow::Result<ToolOutput> {
            Ok(ToolOutput {
                content: "ok".into(),
                is_error: false,
                media: None,
            })
        }
    }

    #[test]
    fn test_register_tool() {
        let mut api = PluginApi::new();
        api.register_tool(Box::new(DummyTool));
        let tools = api.take_tools();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name(), "dummy");
    }

    #[tokio::test]
    async fn test_register_hook() {
        let hooks = Arc::new(HookRegistry::new());
        let mut api = PluginApi::new();
        api.register_hook(
            HookEvent::BeforeToolCall,
            Box::new(|_ctx, _data| Box::pin(async { Ok(HookResult::Continue) })),
        );

        // Apply pending hooks
        for (event, handler) in api.take_hooks() {
            hooks.register(event, handler).await;
        }

        assert_eq!(hooks.count(HookEvent::BeforeToolCall).await, 1);
    }
}
