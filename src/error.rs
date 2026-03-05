#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("ERPNext API error: {message}")]
    Api { message: String },

    #[error("Authentication failed")]
    Auth,

    #[error("Resource not found: {doctype}/{name}")]
    NotFound { doctype: String, name: String },

    #[error("Configuration error: {0}")]
    Config(String),
}
