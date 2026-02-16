//! Generic SSE (Server-Sent Events) line parser.
//!
//! Converts a `reqwest::Response` body into a `Stream<Item = SseEvent>`.

use futures::Stream;
use tokio_stream::StreamExt;

/// A parsed SSE event.
#[derive(Debug, Clone)]
pub struct SseEvent {
    pub event: Option<String>,
    pub data: String,
    pub id: Option<String>,
}

/// Parse a reqwest response body as an SSE stream.
pub fn parse_sse_stream(
    response: reqwest::Response,
) -> impl Stream<Item = anyhow::Result<SseEvent>> {
    let byte_stream = response.bytes_stream();

    // We'll accumulate partial lines across chunks
    futures::stream::unfold(
        SseState {
            byte_stream: Box::pin(byte_stream),
            buffer: String::new(),
            current_event: None,
            current_data: Vec::new(),
            current_id: None,
        },
        |mut state| async move {
            loop {
                // Try to extract a line from the buffer
                if let Some(newline_pos) = state.buffer.find('\n') {
                    let line = state.buffer[..newline_pos].trim_end_matches('\r').to_string();
                    state.buffer = state.buffer[newline_pos + 1..].to_string();

                    if line.is_empty() {
                        // Empty line = dispatch event
                        if !state.current_data.is_empty() {
                            let event = SseEvent {
                                event: state.current_event.take(),
                                data: state.current_data.join("\n"),
                                id: state.current_id.take(),
                            };
                            state.current_data.clear();
                            return Some((Ok(event), state));
                        }
                        continue;
                    }

                    if line.starts_with(':') {
                        // Comment, skip
                        continue;
                    }

                    if let Some(value) = line.strip_prefix("event:") {
                        state.current_event = Some(value.trim_start().to_string());
                    } else if let Some(value) = line.strip_prefix("data:") {
                        state.current_data.push(value.trim_start().to_string());
                    } else if let Some(value) = line.strip_prefix("id:") {
                        state.current_id = Some(value.trim_start().to_string());
                    }
                    // Ignore unknown fields
                    continue;
                }

                // Need more data from the stream
                match state.byte_stream.next().await {
                    Some(Ok(chunk)) => {
                        state.buffer.push_str(&String::from_utf8_lossy(&chunk));
                    }
                    Some(Err(e)) => {
                        return Some((Err(anyhow::anyhow!("SSE stream error: {e}")), state));
                    }
                    None => {
                        // Stream ended. Dispatch any remaining data.
                        if !state.current_data.is_empty() {
                            let event = SseEvent {
                                event: state.current_event.take(),
                                data: state.current_data.join("\n"),
                                id: state.current_id.take(),
                            };
                            state.current_data.clear();
                            return Some((Ok(event), state));
                        }
                        return None;
                    }
                }
            }
        },
    )
}

struct SseState {
    byte_stream: std::pin::Pin<Box<dyn Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send>>,
    buffer: String,
    current_event: Option<String>,
    current_data: Vec<String>,
    current_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sse_event_debug() {
        let event = SseEvent {
            event: Some("message_start".into()),
            data: r#"{"type":"message"}"#.into(),
            id: None,
        };
        assert_eq!(event.event.as_deref(), Some("message_start"));
    }
}
