pub mod config;
pub mod document_source;
pub mod error;
pub mod geometry;
pub mod models;
pub mod namespaces;
pub mod traits;

pub use config::GraphConfig;
pub use document_source::{DocumentSource, ExtractError, Extraction, Section, SourceRef};
pub use error::ConnectorError;
pub use geometry::VectorStoreColumnType;
pub use models::{Document, EntityKind, RagDocument, SyncState};
pub use traits::ContrefortsConnector;

pub type Result<T> = std::result::Result<T, ConnectorError>;
