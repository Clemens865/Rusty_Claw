use thiserror::Error;

#[derive(Debug, Error)]
pub enum RustyClawError {
    #[error("Config error: {0}")]
    Config(String),

    #[error("Gateway error: {0}")]
    Gateway(String),

    #[error("Agent error: {0}")]
    Agent(String),

    #[error("Channel error: {0}")]
    Channel(String),

    #[error("Provider error: {0}")]
    Provider(String),

    #[error("Tool error: {0}")]
    Tool(String),

    #[error("Session error: {0}")]
    Session(String),

    #[error("Auth error: {0}")]
    Auth(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, RustyClawError>;
