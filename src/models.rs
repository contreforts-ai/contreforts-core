use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

/// Generic ERPNext document wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub name: String,
    pub doctype: String,
    pub modified: Option<NaiveDateTime>,
    #[serde(flatten)]
    pub fields: serde_json::Value,
}

/// ERPNext list API response.
#[derive(Debug, Deserialize)]
pub struct ListResponse {
    pub data: Vec<serde_json::Value>,
}

/// ERPNext single-doc API response.
#[derive(Debug, Deserialize)]
pub struct DocResponse {
    pub data: serde_json::Value,
}

/// Metadata about a sync operation for tracking state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncState {
    pub doctype: String,
    pub last_synced: Option<NaiveDateTime>,
    pub count: u64,
}
