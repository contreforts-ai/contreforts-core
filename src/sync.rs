use tracing::info;

use crate::client::ErpNextClient;
use crate::models::SyncState;

/// Sync engine that pulls documents from ERPNext and feeds them to a handler.
pub struct SyncEngine {
    client: ErpNextClient,
}

impl SyncEngine {
    pub fn new(client: ErpNextClient) -> Self {
        Self { client }
    }

    /// Pull all documents of a given doctype, optionally filtering by modification date.
    pub async fn pull(
        &self,
        doctype: &str,
        state: &SyncState,
    ) -> crate::Result<Vec<serde_json::Value>> {
        let filters = state.last_synced.map(|ts| {
            serde_json::json!([["modified", ">", ts.format("%Y-%m-%d %H:%M:%S").to_string()]])
        });

        let docs = self
            .client
            .list(doctype, filters.as_ref(), None, None)
            .await?;

        info!(doctype, count = docs.len(), "pulled documents");
        Ok(docs)
    }

    pub fn client(&self) -> &ErpNextClient {
        &self.client
    }
}
