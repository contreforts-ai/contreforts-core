/// Configuration for the graph store (system-independent).
#[derive(Debug, Clone)]
pub struct GraphConfig {
    pub graph_store_path: String,
}

impl GraphConfig {
    pub fn from_env() -> Result<Self, crate::AdapterError> {
        Ok(Self {
            graph_store_path: std::env::var("GRAPH_STORE_PATH")
                .unwrap_or_else(|_| "./graph_store".into()),
        })
    }
}
