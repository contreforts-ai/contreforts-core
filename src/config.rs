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

#[cfg(test)]
mod tests {
    use super::*;

    /// Env vars are process-global; serialize the tests that mutate them
    /// so parallel runs don't interfere.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn graph_config_from_env_default() {
        let _g = ENV_LOCK.lock().unwrap();
        // SAFETY: tests that touch env are serialized via ENV_LOCK.
        unsafe { std::env::remove_var("GRAPH_STORE_PATH") };
        let cfg = GraphConfig::from_env().unwrap();
        assert_eq!(cfg.graph_store_path, "./graph_store");
    }

    #[test]
    fn graph_config_from_env_override() {
        let _g = ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("GRAPH_STORE_PATH", "/tmp/custom-store") };
        let cfg = GraphConfig::from_env().unwrap();
        assert_eq!(cfg.graph_store_path, "/tmp/custom-store");
        unsafe { std::env::remove_var("GRAPH_STORE_PATH") };
    }
}
