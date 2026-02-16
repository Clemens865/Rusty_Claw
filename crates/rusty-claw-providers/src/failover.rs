//! Failover provider — wraps multiple providers in priority order.
//!
//! On error (rate limit, auth failure, timeout), falls back to the next
//! provider in the list.

use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use futures::Stream;
use tracing::{info, warn};

use crate::{
    CompletionChunk, CompletionRequest, Credentials, LlmProvider, ModelApi, ModelInfo,
    ToolDefinition,
};
use rusty_claw_core::session::TranscriptEntry;

/// A failover provider that tries multiple underlying providers in order.
pub struct FailoverProvider {
    providers: Vec<(Arc<dyn LlmProvider>, Credentials)>,
    label: String,
}

impl FailoverProvider {
    /// Create a failover provider with the given provider/credential pairs.
    /// The first provider is primary; others are fallbacks.
    pub fn new(
        label: String,
        providers: Vec<(Arc<dyn LlmProvider>, Credentials)>,
    ) -> Self {
        Self { providers, label }
    }

    fn primary(&self) -> Option<&(Arc<dyn LlmProvider>, Credentials)> {
        self.providers.first()
    }
}

#[async_trait]
impl LlmProvider for FailoverProvider {
    fn id(&self) -> &str {
        &self.label
    }

    fn api(&self) -> ModelApi {
        // Use the primary provider's API type
        self.primary()
            .map(|(p, _)| p.api())
            .unwrap_or(ModelApi::AnthropicMessages)
    }

    fn format_tools(&self, tools: &[ToolDefinition]) -> Vec<serde_json::Value> {
        self.primary()
            .map(|(p, _)| p.format_tools(tools))
            .unwrap_or_default()
    }

    fn format_messages(&self, transcript: &[TranscriptEntry]) -> Vec<serde_json::Value> {
        self.primary()
            .map(|(p, _)| p.format_messages(transcript))
            .unwrap_or_default()
    }

    fn is_tool_use_stop(&self, stop_reason: &str) -> bool {
        self.primary()
            .is_some_and(|(p, _)| p.is_tool_use_stop(stop_reason))
    }

    async fn stream(
        &self,
        request: &CompletionRequest,
        _credentials: &Credentials,
    ) -> anyhow::Result<Pin<Box<dyn Stream<Item = anyhow::Result<CompletionChunk>> + Send>>> {
        // Try each provider in order using its own credentials.
        let mut last_error = None;

        for (i, (provider, creds)) in self.providers.iter().enumerate() {
            match provider.stream(request, creds).await {
                Ok(stream) => {
                    if i > 0 {
                        info!(
                            provider = provider.id(),
                            attempt = i + 1,
                            "Failover succeeded"
                        );
                    }
                    return Ok(stream);
                }
                Err(e) => {
                    warn!(
                        provider = provider.id(),
                        attempt = i + 1,
                        %e,
                        "Provider failed, trying next"
                    );
                    last_error = Some(e);
                }
            }
        }

        Err(last_error
            .unwrap_or_else(|| anyhow::anyhow!("No providers configured in failover")))
    }

    async fn list_models(
        &self,
        _credentials: &Credentials,
    ) -> anyhow::Result<Vec<ModelInfo>> {
        let mut all_models = Vec::new();
        for (provider, creds) in &self.providers {
            if let Ok(models) = provider.list_models(creds).await {
                all_models.extend(models);
            }
        }
        Ok(all_models)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_failover_provider_creation() {
        let provider = FailoverProvider::new("test-failover".into(), vec![]);
        assert_eq!(provider.id(), "test-failover");
    }

    #[test]
    fn test_failover_format_tools_empty() {
        let provider = FailoverProvider::new("empty".into(), vec![]);
        let tools = provider.format_tools(&[]);
        assert!(tools.is_empty());
    }

    #[test]
    fn test_failover_is_tool_use_stop_empty() {
        let provider = FailoverProvider::new("empty".into(), vec![]);
        assert!(!provider.is_tool_use_stop("tool_use"));
    }
}
