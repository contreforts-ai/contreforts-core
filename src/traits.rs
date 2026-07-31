use chrono::NaiveDateTime;

use crate::error::ConnectorError;
use crate::models::{Document, EntityKind, RagDocument};

/// Uniform interface implemented by every connector (ERPNext, Pennylane, …).
#[async_trait::async_trait]
pub trait ContrefortsConnector: Send + Sync {
    /// Unique lowercase name identifying this adapter ("erpnext", "pennylane", …).
    fn source_name(&self) -> &str;

    /// This connector's own declaration Turtle. `""` means "declares no entity
    /// vocabulary" and keeps the connector in the silent-CORE_NS fallback (case 2).
    ///
    /// Lets a driver holding only `Box<dyn ContrefortsConnector>` values build a real
    /// `EntityDeclarations` from the connectors it already holds, keyed by each
    /// connector's own `source_name()` (contreforts-core#28).
    fn declaration_ttl(&self) -> &'static str {
        ""
    }

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

    /// Pins contreforts-core#28's supply seam: `declaration_ttl()`'s default must be the empty
    /// string, so a connector that does not override it (`NarrowConnector`, standing in for any
    /// third-party connector that predates this method or simply never declares vocabulary)
    /// behaves byte-identically to today -- a true no-op, not merely "doesn't error".
    ///
    /// CORRECTED 2026-08-01 (contreforts-kg#45, reported via kg#45's comments 2026-07-31 while
    /// verifying contreforts-core#28): this paragraph used to end with "This is currently a
    /// compile error (`declaration_ttl` does not exist on `ContrefortsConnector` yet), which is
    /// the sanctioned RED per contreforts-kg/CONTRIBUTING.md#3". That was true when a1 wrote it
    /// for this method's RED phase and false the moment a2 implemented `declaration_ttl` (`:18`
    /// above, eighteen lines above this comment) in the same chain -- contreforts-core#29 merged
    /// with the claim left unrevised. `declaration_ttl` exists today; this test now exercises a
    /// real default, not a compile failure.
    ///
    /// Mutation-proof: if the default were ever anything other than `""` (e.g. some sentinel, or
    /// `None`-like placeholder text), this assertion -- comparing against the literal empty
    /// string, not against any constant the implementation defines -- fails naming the exact
    /// wrong value returned.
    #[test]
    fn connector_that_does_not_override_declaration_ttl_gets_the_empty_no_op_default() {
        let connector = NarrowConnector;

        assert_eq!(
            connector.declaration_ttl(),
            "",
            "a connector that does not override declaration_ttl() must get the empty-string \
             default (contreforts-core#28) -- got {:?} instead, which is not the documented \
             no-op and would change behaviour for every connector that predates this method",
            connector.declaration_ttl()
        );
    }
}
