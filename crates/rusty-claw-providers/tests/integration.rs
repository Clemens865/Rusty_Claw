//! Provider integration tests — real API calls.
//!
//! These tests are skipped when the corresponding API key env var is not set.
//! Run with: `cargo test -p rusty-claw-providers --test integration`

use rusty_claw_providers::{CompletionRequest, Credentials, LlmProvider};
use tokio_stream::StreamExt;

fn anthropic_key() -> Option<String> {
    std::env::var("ANTHROPIC_API_KEY")
        .ok()
        .filter(|k| !k.is_empty())
}

fn openai_key() -> Option<String> {
    std::env::var("OPENAI_API_KEY")
        .ok()
        .filter(|k| !k.is_empty())
}

fn google_key() -> Option<String> {
    std::env::var("GOOGLE_AI_API_KEY")
        .ok()
        .filter(|k| !k.is_empty())
}

/// Helper to run a simple streaming completion and verify we get text back.
async fn verify_simple_completion(
    provider: &dyn LlmProvider,
    credentials: &Credentials,
    model: &str,
) {
    let messages = provider.format_messages(&[
        rusty_claw_core::session::TranscriptEntry::User {
            content: vec![rusty_claw_core::types::ContentBlock::Text {
                text: "Reply with exactly the word 'hello'.".into(),
            }],
            timestamp: chrono::Utc::now(),
        },
    ]);

    let request = CompletionRequest {
        model: model.to_string(),
        messages,
        max_tokens: 50,
        temperature: Some(0.0),
        tools: None,
        system: Some("You are a helpful assistant. Follow instructions exactly.".into()),
        thinking_budget_tokens: None,
    };

    let stream = provider.stream(&request, credentials).await;
    assert!(stream.is_ok(), "Stream creation failed: {:?}", stream.err());

    let mut stream = std::pin::pin!(stream.unwrap());
    let mut text = String::new();
    let mut got_chunks = false;

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.expect("Stream chunk error");
        if let Some(delta) = &chunk.delta {
            text.push_str(delta);
            got_chunks = true;
        }
    }

    assert!(got_chunks, "No text chunks received");
    assert!(
        text.to_lowercase().contains("hello"),
        "Expected 'hello' in response, got: {text}"
    );
}

#[tokio::test]
async fn test_anthropic_streaming() {
    let Some(api_key) = anthropic_key() else {
        eprintln!("Skipping: ANTHROPIC_API_KEY not set");
        return;
    };

    let provider = rusty_claw_providers::anthropic::AnthropicProvider::new(None);
    let credentials = Credentials::ApiKey { api_key };

    verify_simple_completion(&provider, &credentials, "claude-sonnet-4-5-20250929").await;
}

#[tokio::test]
async fn test_openai_streaming() {
    let Some(api_key) = openai_key() else {
        eprintln!("Skipping: OPENAI_API_KEY not set");
        return;
    };

    let provider = rusty_claw_providers::openai::OpenAiProvider::openai(None);
    let credentials = Credentials::ApiKey { api_key };

    verify_simple_completion(&provider, &credentials, "gpt-4o-mini").await;
}

#[tokio::test]
async fn test_google_streaming() {
    let Some(api_key) = google_key() else {
        eprintln!("Skipping: GOOGLE_AI_API_KEY not set");
        return;
    };

    let provider = rusty_claw_providers::google::GeminiProvider::new(None);
    let credentials = Credentials::ApiKey { api_key };

    verify_simple_completion(&provider, &credentials, "gemini-2.0-flash").await;
}

#[tokio::test]
async fn test_anthropic_model_list() {
    let Some(api_key) = anthropic_key() else {
        eprintln!("Skipping: ANTHROPIC_API_KEY not set");
        return;
    };

    let provider = rusty_claw_providers::anthropic::AnthropicProvider::new(None);
    let credentials = Credentials::ApiKey { api_key };

    let models = provider.list_models(&credentials).await;
    assert!(models.is_ok(), "Model list failed: {:?}", models.err());

    let models = models.unwrap();
    assert!(!models.is_empty(), "No models returned");
}
