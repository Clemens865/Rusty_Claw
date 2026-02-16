//! Hook system — register async handlers for lifecycle events.
//!
//! Hooks allow plugins to observe and modify agent behavior at well-defined
//! points in the lifecycle (before/after tool calls, LLM input/output, etc.).

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use chrono::{DateTime, Utc};
use tokio::sync::RwLock;

use crate::HookEvent;

/// Context passed to every hook handler invocation.
#[derive(Debug, Clone)]
pub struct HookContext {
    pub session_key: String,
    pub timestamp: DateTime<Utc>,
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Result returned by a hook handler.
#[derive(Debug)]
pub enum HookResult {
    /// Continue with the original data unchanged.
    Continue,
    /// Continue with modified data (fed to the next handler in the chain).
    Modified(serde_json::Value),
    /// Cancel the operation with a reason string.
    Cancel(String),
}

/// Async hook handler function type.
pub type HookHandler = Box<
    dyn Fn(HookContext, serde_json::Value) -> Pin<Box<dyn Future<Output = anyhow::Result<HookResult>> + Send>>
        + Send
        + Sync,
>;

/// Registry of hook handlers, keyed by event type.
pub struct HookRegistry {
    handlers: RwLock<HashMap<HookEvent, Vec<HookHandler>>>,
}

impl HookRegistry {
    pub fn new() -> Self {
        Self {
            handlers: RwLock::new(HashMap::new()),
        }
    }

    /// Register a handler for a specific hook event.
    pub async fn register(&self, event: HookEvent, handler: HookHandler) {
        let mut handlers = self.handlers.write().await;
        handlers.entry(event).or_default().push(handler);
    }

    /// Fire all handlers for an event in registration order.
    pub async fn fire(
        &self,
        event: HookEvent,
        ctx: HookContext,
        data: serde_json::Value,
    ) -> HookResult {
        let handlers = self.handlers.read().await;
        let Some(chain) = handlers.get(&event) else {
            return HookResult::Continue;
        };

        let mut current = data;
        for handler in chain {
            match handler(ctx.clone(), current.clone()).await {
                Ok(HookResult::Continue) => {}
                Ok(HookResult::Modified(new_data)) => {
                    current = new_data;
                }
                Ok(HookResult::Cancel(reason)) => {
                    return HookResult::Cancel(reason);
                }
                Err(e) => {
                    tracing::warn!(event = ?event, error = %e, "Hook handler error, continuing");
                }
            }
        }

        HookResult::Continue
    }

    /// Fire hooks, returning the (potentially modified) data.
    ///
    /// Returns the final data value, or an error string if cancelled.
    pub async fn fire_or_cancel(
        &self,
        event: HookEvent,
        ctx: HookContext,
        data: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let handlers = self.handlers.read().await;
        let Some(chain) = handlers.get(&event) else {
            return Ok(data);
        };

        let mut current = data;
        for handler in chain {
            match handler(ctx.clone(), current.clone()).await {
                Ok(HookResult::Continue) => {}
                Ok(HookResult::Modified(new_data)) => {
                    current = new_data;
                }
                Ok(HookResult::Cancel(reason)) => {
                    return Err(reason);
                }
                Err(e) => {
                    tracing::warn!(event = ?event, error = %e, "Hook handler error, continuing");
                }
            }
        }

        Ok(current)
    }

    /// Return the number of handlers registered for a given event.
    pub async fn count(&self, event: HookEvent) -> usize {
        let handlers = self.handlers.read().await;
        handlers.get(&event).map_or(0, |v| v.len())
    }
}

impl Default for HookRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn test_ctx() -> HookContext {
        HookContext {
            session_key: "test-session".into(),
            timestamp: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn test_empty_registry_returns_continue() {
        let registry = HookRegistry::new();
        let result = registry
            .fire(HookEvent::BeforeAgentStart, test_ctx(), serde_json::json!({}))
            .await;
        assert!(matches!(result, HookResult::Continue));
    }

    #[tokio::test]
    async fn test_fire_order() {
        let registry = HookRegistry::new();
        let order = Arc::new(tokio::sync::Mutex::new(Vec::new()));

        let order1 = order.clone();
        registry
            .register(
                HookEvent::BeforeAgentStart,
                Box::new(move |_ctx, data| {
                    let order = order1.clone();
                    Box::pin(async move {
                        order.lock().await.push(1);
                        Ok(HookResult::Modified(data))
                    })
                }),
            )
            .await;

        let order2 = order.clone();
        registry
            .register(
                HookEvent::BeforeAgentStart,
                Box::new(move |_ctx, data| {
                    let order = order2.clone();
                    Box::pin(async move {
                        order.lock().await.push(2);
                        Ok(HookResult::Modified(data))
                    })
                }),
            )
            .await;

        registry
            .fire(HookEvent::BeforeAgentStart, test_ctx(), serde_json::json!({}))
            .await;

        let recorded = order.lock().await;
        assert_eq!(*recorded, vec![1, 2]);
    }

    #[tokio::test]
    async fn test_cancel_stops_chain() {
        let registry = HookRegistry::new();
        let second_ran = Arc::new(tokio::sync::Mutex::new(false));

        registry
            .register(
                HookEvent::BeforeToolCall,
                Box::new(|_ctx, _data| {
                    Box::pin(async { Ok(HookResult::Cancel("blocked".into())) })
                }),
            )
            .await;

        let flag = second_ran.clone();
        registry
            .register(
                HookEvent::BeforeToolCall,
                Box::new(move |_ctx, data| {
                    let flag = flag.clone();
                    Box::pin(async move {
                        *flag.lock().await = true;
                        Ok(HookResult::Modified(data))
                    })
                }),
            )
            .await;

        let result = registry
            .fire_or_cancel(HookEvent::BeforeToolCall, test_ctx(), serde_json::json!({}))
            .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "blocked");
        assert!(!*second_ran.lock().await);
    }

    #[tokio::test]
    async fn test_modified_chains() {
        let registry = HookRegistry::new();

        registry
            .register(
                HookEvent::LlmInput,
                Box::new(|_ctx, mut data| {
                    Box::pin(async move {
                        data["step1"] = serde_json::json!(true);
                        Ok(HookResult::Modified(data))
                    })
                }),
            )
            .await;

        registry
            .register(
                HookEvent::LlmInput,
                Box::new(|_ctx, mut data| {
                    Box::pin(async move {
                        assert_eq!(data["step1"], serde_json::json!(true));
                        data["step2"] = serde_json::json!(true);
                        Ok(HookResult::Modified(data))
                    })
                }),
            )
            .await;

        let result = registry
            .fire_or_cancel(HookEvent::LlmInput, test_ctx(), serde_json::json!({}))
            .await
            .unwrap();

        assert_eq!(result["step1"], serde_json::json!(true));
        assert_eq!(result["step2"], serde_json::json!(true));
    }

    #[tokio::test]
    async fn test_count() {
        let registry = HookRegistry::new();
        assert_eq!(registry.count(HookEvent::AgentEnd).await, 0);

        registry
            .register(
                HookEvent::AgentEnd,
                Box::new(|_ctx, _data| Box::pin(async { Ok(HookResult::Continue) })),
            )
            .await;
        assert_eq!(registry.count(HookEvent::AgentEnd).await, 1);

        registry
            .register(
                HookEvent::AgentEnd,
                Box::new(|_ctx, _data| Box::pin(async { Ok(HookResult::Continue) })),
            )
            .await;
        assert_eq!(registry.count(HookEvent::AgentEnd).await, 2);
    }
}
