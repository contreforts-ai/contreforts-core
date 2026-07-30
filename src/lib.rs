pub mod config;
pub mod error;
pub mod models;
pub mod namespaces;
pub mod traits;

pub use config::GraphConfig;
pub use error::ConnectorError;
pub use models::{Document, EntityKind, RagDocument, SyncState};
pub use traits::ContrefortsConnector;

pub type Result<T> = std::result::Result<T, ConnectorError>;
