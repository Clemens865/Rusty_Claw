//! Anthropic Messages API provider.
//!
//! Implements streaming chat completions via the Anthropic Messages API.
//! This is the primary provider for Claude models.

// TODO: Implement AnthropicProvider
// - POST /v1/messages with stream: true
// - SSE event parsing (message_start, content_block_delta, message_delta, message_stop)
// - Tool use response handling
// - Thinking/reasoning block support
