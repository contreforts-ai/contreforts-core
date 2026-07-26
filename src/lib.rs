pub mod config;
pub mod error;
pub mod models;
pub mod traits;

pub use config::GraphConfig;
pub use error::ConnectorError;
pub use models::{Document, EntityKind, PENNYLANE_UNSUPPORTED, RagDocument, SyncState};
pub use traits::ContrefortsConnector;

#[deprecated(note = "renamed to ConnectorError; see contreforts-workspace#1")]
pub use ConnectorError as AdapterError;
#[deprecated(note = "renamed to ContrefortsConnector; see contreforts-workspace#1")]
pub use ContrefortsConnector as ErpAdapter;

pub type Result<T> = std::result::Result<T, ConnectorError>;
