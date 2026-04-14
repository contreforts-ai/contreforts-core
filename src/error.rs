#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error("HTTP error: {0}")]
    Http(String),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("API error: {message}")]
    Api { message: String },

    #[error("Authentication failed")]
    Auth,

    #[error("Not found: {kind}/{id}")]
    NotFound { kind: String, id: String },

    #[error("Configuration error: {0}")]
    Config(String),
}
