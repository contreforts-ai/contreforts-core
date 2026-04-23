pub mod config;
pub mod error;
pub mod models;
pub mod traits;

pub use config::GraphConfig;
pub use error::AdapterError;
pub use models::{Document, EntityKind, PENNYLANE_UNSUPPORTED, SyncState};
pub use traits::ErpAdapter;

pub type Result<T> = std::result::Result<T, AdapterError>;
