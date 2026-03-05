use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub erpnext_url: String,
    pub api_key: String,
    pub api_secret: String,
    /// Path to the Oxigraph store directory
    pub graph_store_path: String,
}

impl Config {
    pub fn from_env() -> crate::Result<Self> {
        Ok(Self {
            erpnext_url: env_var("ERPNEXT_URL")?,
            api_key: env_var("ERPNEXT_API_KEY")?,
            api_secret: env_var("ERPNEXT_API_SECRET")?,
            graph_store_path: env_var("GRAPH_STORE_PATH")
                .unwrap_or_else(|_| "./graph_store".into()),
        })
    }
}

fn env_var(name: &str) -> crate::Result<String> {
    std::env::var(name).map_err(|_| crate::Error::Config(format!("{name} not set")))
}
