use chrono::NaiveDateTime;

use crate::error::AdapterError;
use crate::models::{Document, EntityKind};

/// Uniform interface implemented by every ERP adapter (ERPNext, Pennylane, …).
#[async_trait::async_trait]
pub trait ErpAdapter: Send + Sync {
    /// Unique lowercase name identifying this adapter ("erpnext", "pennylane", …).
    fn source_name(&self) -> &str;

    /// Pull all entities of `kind`, optionally filtered to records modified after `since`.
    async fn pull(
        &self,
        kind: EntityKind,
        since: Option<NaiveDateTime>,
    ) -> Result<Vec<Document>, AdapterError>;

    /// Push a new entity to the remote system.
    /// Returns the created `Document` with `remote_id` populated.
    async fn push(&self, doc: &Document) -> Result<Document, AdapterError>;

    /// Fetch a single entity by its remote-system ID.
    async fn get(&self, kind: EntityKind, remote_id: &str) -> Result<Document, AdapterError>;
}
