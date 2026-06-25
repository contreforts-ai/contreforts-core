//! Graph store configuration and per-user store resolution.

use std::ffi::OsString;
use std::path::PathBuf;

/// Configuration for the graph store (system-independent).
///
/// # Per-user store layout
///
/// A single Oxigraph store holds **both** graphs for one user:
/// - the **configuration graph** (`erpd:graph/config` — companies, connectors,
///   the `me` profile), and
/// - the **business / knowledge data** (per-company named graphs + default graph).
///
/// The two are already separated by named-graph IRI *inside* the store, so
/// per-user isolation is achieved simply by giving each user a distinct store
/// *directory* — no graph-splitting is required.
///
/// # Resolution precedence
///
/// [`GraphConfig::from_env`] resolves the store directory as:
/// 1. `GRAPH_STORE_PATH` env var — explicit override for headless / server / CI use.
/// 2. The per-user OS data directory — `<data_dir>/erp-sync/graph_store`
///    (`~/.local/share` on Linux, `~/Library/Application Support` on macOS,
///    `%APPDATA%` on Windows). This isolates each OS user's data by default.
/// 3. `./graph_store` relative to the cwd — last-resort fallback when no OS data
///    directory can be determined.
///
/// The Tauri desktop app does not go through this resolver: it derives the store
/// from Tauri's per-app `app_data_dir` and passes it in explicitly (no login).
#[derive(Debug, Clone)]
pub struct GraphConfig {
    pub graph_store_path: String,
}

impl GraphConfig {
    /// Resolve the store path following the documented precedence
    /// (`GRAPH_STORE_PATH` override → per-user OS data dir → `./graph_store`).
    pub fn from_env() -> Result<Self, crate::AdapterError> {
        Ok(Self {
            graph_store_path: resolve_store_path(std::env::var_os("GRAPH_STORE_PATH")),
        })
    }

    /// The per-user default store directory, ignoring any `GRAPH_STORE_PATH`
    /// override: `<os-data-dir>/erp-sync/graph_store`, or `./graph_store` when no
    /// OS data directory can be determined.
    pub fn per_user_default() -> PathBuf {
        per_user_store_dir().unwrap_or_else(|| PathBuf::from("./graph_store"))
    }
}

/// Apply the resolution precedence given the (possibly unset/empty) override.
/// Split out from [`GraphConfig::from_env`] so it can be unit-tested without
/// mutating the process environment.
fn resolve_store_path(override_var: Option<OsString>) -> String {
    if let Some(val) = override_var
        && !val.is_empty()
    {
        return val.to_string_lossy().into_owned();
    }
    GraphConfig::per_user_default()
        .to_string_lossy()
        .into_owned()
}

fn per_user_store_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("erp-sync").join("graph_store"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Env vars are process-global; serialize the tests that mutate them
    /// so parallel runs don't interfere.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn resolve_prefers_explicit_override() {
        // A non-empty override always wins, regardless of the OS data dir.
        let got = resolve_store_path(Some(OsString::from("/tmp/custom-store")));
        assert_eq!(got, "/tmp/custom-store");
    }

    #[test]
    fn resolve_treats_empty_override_as_unset() {
        // `GRAPH_STORE_PATH=` must not pin the store to an empty path.
        let got = resolve_store_path(Some(OsString::new()));
        assert_eq!(got, GraphConfig::per_user_default().to_string_lossy());
    }

    #[test]
    fn resolve_falls_back_to_per_user_default() {
        let got = resolve_store_path(None);
        assert_eq!(got, GraphConfig::per_user_default().to_string_lossy());
    }

    #[test]
    fn per_user_default_is_under_the_os_data_dir() {
        // On any platform with a resolvable data dir, the default lives under it
        // and is no longer the legacy bare `./graph_store`.
        if let Some(data) = dirs::data_dir() {
            let def = GraphConfig::per_user_default();
            assert!(def.starts_with(&data), "{def:?} not under {data:?}");
            assert!(def.ends_with("graph_store"));
            assert_ne!(def, PathBuf::from("./graph_store"));
        }
    }

    #[test]
    fn from_env_override_wins() {
        let _g = ENV_LOCK.lock().unwrap();
        // SAFETY: tests that touch env are serialized via ENV_LOCK.
        unsafe { std::env::set_var("GRAPH_STORE_PATH", "/tmp/custom-store") };
        let cfg = GraphConfig::from_env().unwrap();
        assert_eq!(cfg.graph_store_path, "/tmp/custom-store");
        unsafe { std::env::remove_var("GRAPH_STORE_PATH") };
    }

    #[test]
    fn from_env_default_is_per_user() {
        let _g = ENV_LOCK.lock().unwrap();
        unsafe { std::env::remove_var("GRAPH_STORE_PATH") };
        let cfg = GraphConfig::from_env().unwrap();
        assert_eq!(
            cfg.graph_store_path,
            GraphConfig::per_user_default().to_string_lossy()
        );
    }
}
