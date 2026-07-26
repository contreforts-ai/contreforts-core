use chrono::NaiveDateTime;

use crate::error::ConnectorError;
use crate::models::{Document, EntityKind, RagDocument};

/// Uniform interface implemented by every connector (ERPNext, Pennylane, …).
#[async_trait::async_trait]
pub trait ContrefortsConnector: Send + Sync {
    /// Unique lowercase name identifying this adapter ("erpnext", "pennylane", …).
    fn source_name(&self) -> &str;

    /// Pull all entities of `kind`, optionally filtered to records modified after `since`.
    async fn pull(
        &self,
        kind: EntityKind,
        since: Option<NaiveDateTime>,
    ) -> Result<Vec<Document>, ConnectorError>;

    /// Push a new entity to the remote system.
    /// Returns the created `Document` with `remote_id` populated.
    async fn push(&self, doc: &Document) -> Result<Document, ConnectorError>;

    /// Update an existing entity in the remote system using `doc.remote_id` as the key.
    /// Returns the updated `Document`. Adapters that do not support updates should return
    /// `ConnectorError::Api { message: "update not supported".into() }`.
    async fn update(&self, doc: &Document) -> Result<Document, ConnectorError> {
        let _ = doc;
        Err(ConnectorError::Api {
            message: format!("update not supported by adapter '{}'", self.source_name()),
        })
    }

    /// Fetch a single entity by its remote-system ID.
    async fn get(&self, kind: EntityKind, remote_id: &str) -> Result<Document, ConnectorError>;

    /// Fetch authoritative free-form content for `entity_iri` from the remote system
    /// for RAG ingestion (issue body + comments, document text, etc.). Returns one
    /// `RagDocument` per structural part so the indexer can chunk and embed them
    /// independently while preserving the entity → graph linkage.
    ///
    /// Default impl returns `Unsupported`; adapters opt in by overriding.
    async fn fetch_content(
        &self,
        entity_iri: &str,
        kind: EntityKind,
    ) -> Result<Vec<RagDocument>, ConnectorError> {
        let _ = (entity_iri, kind);
        Err(ConnectorError::Unsupported {
            connector: self.source_name().to_string(),
            operation: "fetch_content".to_string(),
        })
    }
}
