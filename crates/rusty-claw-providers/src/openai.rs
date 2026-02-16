//! OpenAI Completions/Responses API provider.
//!
//! Implements streaming chat completions via OpenAI's API.
//! Also serves as the base for OpenRouter and other OpenAI-compatible providers.

// TODO: Implement OpenAiProvider
// - POST /v1/chat/completions with stream: true
// - SSE event parsing
// - Tool call handling (function_call / tool_calls)
// - Compatible with OpenRouter, Together, Groq, etc.
