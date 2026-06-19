use std::path::{Path, PathBuf};

/// Source of configuration values — allows swapping hardcoded defaults
/// for user-config-file values later without changing consumers.
pub trait ConfigSource {
    fn paypunkd_socket_path(&self) -> &str;
    fn keypunkd_socket_path(&self) -> &str;
    fn data_dir(&self) -> &Path;
    fn config_dir(&self) -> &Path;
    fn rpc_url(&self) -> &str;
    fn db_password(&self) -> &str;
}

/// Hardcoded default configuration.
///
/// All values are compile-time constants. Replace the implementation
/// of ConfigSource to read from ~/.config/paypunk/config.toml later.
pub struct HardcodedConfig;

impl HardcodedConfig {
    fn home_dir() -> PathBuf {
        PathBuf::from(
            std::env::var("HOME")
                .expect("HOME environment variable must be set"),
        )
    }
}

impl ConfigSource for HardcodedConfig {
    fn paypunkd_socket_path(&self) -> &str {
        "/tmp/paypunkd.sock"
    }

    fn keypunkd_socket_path(&self) -> &str {
        "/tmp/keypunkd.sock"
    }

    fn data_dir(&self) -> &Path {
        // Using a leak to return a static &Path from a runtime-computed value.
        // This is acceptable for a hardcoded config that is constructed once at startup.
        let path = Self::home_dir().join(".local/share/paypunk/");
        // Ensure the path string outlives the function
        Box::leak(path.into_boxed_path())
    }

    fn config_dir(&self) -> &Path {
        let path = Self::home_dir().join(".config/paypunk/");
        Box::leak(path.into_boxed_path())
    }

    fn rpc_url(&self) -> &str {
        "http://127.0.0.1:8545"
    }

    fn db_password(&self) -> &str {
        "paypunk-default-password"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hardcoded_config_defaults() {
        let config = HardcodedConfig;
        assert!(config.paypunkd_socket_path().contains("paypunkd.sock"));
        assert!(config.keypunkd_socket_path().contains("keypunkd.sock"));
        assert!(config.data_dir().to_string_lossy().contains("paypunk"));
    }

    #[test]
    fn test_config_source_trait() {
        let config: &dyn ConfigSource = &HardcodedConfig;
        assert!(!config.paypunkd_socket_path().is_empty());
    }
}
