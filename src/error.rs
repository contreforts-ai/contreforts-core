#[derive(Debug, thiserror::Error)]
pub enum ConnectorError {
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

    #[error("Operation not supported by connector '{connector}': {operation}")]
    Unsupported {
        connector: String,
        operation: String,
    },
}

#[cfg(feature = "reqwest")]
impl From<reqwest::Error> for ConnectorError {
    fn from(e: reqwest::Error) -> Self {
        if e.status()
            .map(|s| s.as_u16() == 401 || s.as_u16() == 403)
            .unwrap_or(false)
        {
            ConnectorError::Auth
        } else {
            ConnectorError::Http(e.to_string())
        }
    }
}
