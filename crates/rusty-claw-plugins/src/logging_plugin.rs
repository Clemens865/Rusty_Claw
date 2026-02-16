//! Example logging plugin — logs agent lifecycle events via `tracing`.

use crate::api::PluginApi;
use crate::hooks::HookResult;
use crate::HookEvent;
use crate::Plugin;

/// A simple plugin that logs lifecycle events for debugging and demonstration.
pub struct LoggingPlugin;

impl Plugin for LoggingPlugin {
    fn id(&self) -> &str {
        "builtin.logging"
    }

    fn name(&self) -> &str {
        "Logging Plugin"
    }

    fn register(&self, api: &mut PluginApi) {
        api.register_hook(
            HookEvent::BeforeAgentStart,
            Box::new(|ctx, data| {
                Box::pin(async move {
                    tracing::info!(
                        session = %ctx.session_key,
                        "Hook: BeforeAgentStart — agent run starting"
                    );
                    let _ = &data;
                    Ok(HookResult::Continue)
                })
            }),
        );

        api.register_hook(
            HookEvent::BeforeToolCall,
            Box::new(|ctx, data| {
                Box::pin(async move {
                    let tool = data
                        .get("tool")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    tracing::info!(
                        session = %ctx.session_key,
                        tool = tool,
                        "Hook: BeforeToolCall"
                    );
                    Ok(HookResult::Continue)
                })
            }),
        );

        api.register_hook(
            HookEvent::AgentEnd,
            Box::new(|ctx, data| {
                Box::pin(async move {
                    let duration = data
                        .get("duration_ms")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    tracing::info!(
                        session = %ctx.session_key,
                        duration_ms = duration,
                        "Hook: AgentEnd — agent run completed"
                    );
                    Ok(HookResult::Continue)
                })
            }),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_logging_plugin_registers_hooks() {
        let plugin = LoggingPlugin;
        let mut api = PluginApi::new();
        plugin.register(&mut api);

        let hooks = api.take_hooks();
        assert_eq!(hooks.len(), 3);

        let events: Vec<_> = hooks.iter().map(|(e, _)| *e).collect();
        assert!(events.contains(&HookEvent::BeforeAgentStart));
        assert!(events.contains(&HookEvent::BeforeToolCall));
        assert!(events.contains(&HookEvent::AgentEnd));
    }
}
