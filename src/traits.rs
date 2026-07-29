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

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal connector that only recognises `EntityKind::CUSTOMER`, standing in for
    /// any of the six real connectors. Exercises the fallback policy documented next to
    /// `EntityKind` (`contreforts/contreforts-core#18`): a kind this connector does not
    /// handle must error, naming both the kind and the connector -- never silently
    /// return empty.
    struct NarrowConnector;

    #[async_trait::async_trait]
    impl ContrefortsConnector for NarrowConnector {
        fn source_name(&self) -> &str {
            "narrow"
        }

        async fn pull(
            &self,
            kind: EntityKind,
            _since: Option<NaiveDateTime>,
        ) -> Result<Vec<Document>, ConnectorError> {
            if kind == EntityKind::CUSTOMER {
                return Ok(vec![]);
            }
            Err(ConnectorError::UnsupportedKind {
                connector: self.source_name().to_string(),
                kind: kind.as_str().to_string(),
            })
        }

        async fn get(
            &self,
            kind: EntityKind,
            _remote_id: &str,
        ) -> Result<Document, ConnectorError> {
            Err(ConnectorError::UnsupportedKind {
                connector: self.source_name().to_string(),
                kind: kind.as_str().to_string(),
            })
        }

        async fn push(&self, doc: &Document) -> Result<Document, ConnectorError> {
            Err(ConnectorError::UnsupportedKind {
                connector: self.source_name().to_string(),
                kind: doc.kind.as_str().to_string(),
            })
        }
    }

    #[tokio::test]
    async fn unhandled_kind_errors_naming_kind_and_connector() {
        let connector = NarrowConnector;

        let err = connector
            .pull(EntityKind::new("RiskScenario"), None)
            .await
            .expect_err("a kind this connector does not handle must error");

        match err {
            ConnectorError::UnsupportedKind { connector, kind } => {
                assert_eq!(connector, "narrow");
                assert_eq!(kind, "RiskScenario");
            }
            other => panic!("expected UnsupportedKind, got {other:?}"),
        }
    }

    /// Mutation-proof for the fallback policy: a connector that took the "return empty
    /// on an unhandled kind" shortcut instead of erroring must fail this test. Asserting
    /// only `is_err()` (not matching the specific variant) is what catches an
    /// `Ok(vec![])` fallback -- the exact failure mode this policy exists to rule out.
    #[tokio::test]
    async fn unhandled_kind_is_never_silently_empty() {
        let connector = NarrowConnector;

        let result = connector.pull(EntityKind::new("RiskScenario"), None).await;

        assert!(
            result.is_err(),
            "connector returned {result:?} for an unhandled kind instead of erroring"
        );
    }

    #[tokio::test]
    async fn handled_kind_still_succeeds() {
        let connector = NarrowConnector;

        let result = connector.pull(EntityKind::CUSTOMER, None).await;

        assert!(result.is_ok(), "a recognised kind must not error");
    }
}
