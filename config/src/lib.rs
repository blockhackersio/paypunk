use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PaypunkConfig {
    #[serde(default = "default_paypunkd_socket_path")]
    pub paypunkd_socket_path: String,
    #[serde(default = "default_keypunkd_socket_path")]
    pub keypunkd_socket_path: String,
    #[serde(default = "default_data_dir")]
    pub data_dir: String,
    #[serde(default = "default_config_dir")]
    pub config_dir: String,
    #[serde(default = "default_ethereum_rpc_url")]
    pub ethereum_rpc_url: String,
    #[serde(default = "default_lightwalletd_host")]
    pub lightwalletd_host: String,
    #[serde(default = "default_zcash_network")]
    pub zcash_network: String,
    #[serde(default = "default_bridge_socket_path")]
    pub bridge_socket_path: String,
    #[serde(default = "default_offline_signer")]
    pub offline_signer: bool,
}

fn default_paypunkd_socket_path() -> String {
    "/tmp/paypunkd.sock".to_string()
}

fn default_keypunkd_socket_path() -> String {
    "/tmp/keypunkd.sock".to_string()
}

fn default_data_dir() -> String {
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("~/.local/share"));
    base.join("paypunk").to_string_lossy().to_string()
}

fn default_config_dir() -> String {
    let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("~/.config"));
    base.join("paypunk").to_string_lossy().to_string()
}

fn default_ethereum_rpc_url() -> String {
    "http://127.0.0.1:8545".to_string()
}

fn default_lightwalletd_host() -> String {
    "http://127.0.0.1:9067".to_string()
}

fn default_zcash_network() -> String {
    "regtest".to_string()
}

fn default_bridge_socket_path() -> String {
    "/tmp/paypunk-bridge.sock".to_string()
}

fn default_offline_signer() -> bool {
    false
}

impl Default for PaypunkConfig {
    fn default() -> Self {
        Self {
            paypunkd_socket_path: default_paypunkd_socket_path(),
            keypunkd_socket_path: default_keypunkd_socket_path(),
            data_dir: default_data_dir(),
            config_dir: default_config_dir(),
            ethereum_rpc_url: default_ethereum_rpc_url(),
            lightwalletd_host: default_lightwalletd_host(),
            zcash_network: default_zcash_network(),
            bridge_socket_path: default_bridge_socket_path(),
            offline_signer: default_offline_signer(),
        }
    }
}

pub struct ConfigLoader;

impl ConfigLoader {
    pub fn load() -> Result<PaypunkConfig, ConfigError> {
        let config_dir = dirs::config_dir()
            .ok_or(ConfigError::NoConfigDir)?
            .join("paypunk");
        let config_path = config_dir.join("config.toml");

        if !config_path.exists() {
            return Err(ConfigError::NotFound(config_path));
        }

        let contents = std::fs::read_to_string(&config_path)
            .map_err(|e| ConfigError::Io(config_path.clone(), e))?;

        let mut config: PaypunkConfig =
            toml::from_str(&contents).map_err(|e| ConfigError::Parse(config_path.clone(), e))?;

        Self::apply_env_overrides(&mut config);

        Ok(config)
    }

    pub fn load_or_default() -> PaypunkConfig {
        let config_dir = match dirs::config_dir() {
            Some(d) => d.join("paypunk"),
            None => return PaypunkConfig::default(),
        };
        let config_path = config_dir.join("config.toml");

        if !config_path.exists() {
            return PaypunkConfig::default();
        }

        let contents = match std::fs::read_to_string(&config_path) {
            Ok(c) => c,
            Err(_) => return PaypunkConfig::default(),
        };

        let mut config: PaypunkConfig = match toml::from_str(&contents) {
            Ok(c) => c,
            Err(_) => return PaypunkConfig::default(),
        };

        Self::apply_env_overrides(&mut config);

        config
    }

    pub fn write_default() -> Result<(), ConfigError> {
        let config_dir = dirs::config_dir()
            .ok_or(ConfigError::NoConfigDir)?
            .join("paypunk");
        std::fs::create_dir_all(&config_dir).map_err(|e| ConfigError::Io(config_dir.clone(), e))?;

        let config_path = config_dir.join("config.toml");
        let contents = r#"# Paypunk Configuration
#
# Socket paths
paypunkd_socket_path = "/tmp/paypunkd.sock"
keypunkd_socket_path = "/tmp/keypunkd.sock"
bridge_socket_path = "/tmp/paypunk-bridge.sock"

# Data and config directories
data_dir = "~/.local/share/paypunk/"
config_dir = "~/.config/paypunk/"

# RPC URL for Ethereum-compatible chains
ethereum_rpc_url = "http://127.0.0.1:8545"

# Zcash lightwalletd host (default: http://127.0.0.1:9067)
lightwalletd_host = "http://127.0.0.1:9067"

# Zcash network (regtest, testnet, or mainnet)
zcash_network = "regtest"

# Offline signer mode (default: false)
# When true, spawns the QR bridge instead of keypunkd
offline_signer = false
"#;

        std::fs::write(&config_path, contents)
            .map_err(|e| ConfigError::Io(config_path.clone(), e))?;

        Ok(())
    }

    pub(crate) fn apply_env_overrides(config: &mut PaypunkConfig) {
        if let Ok(v) = std::env::var("PAYPUNK_SOCKET_PATH") {
            config.paypunkd_socket_path = v;
        }
        if let Ok(v) = std::env::var("KEYPUNKD_SOCKET_PATH") {
            config.keypunkd_socket_path = v;
        }
        if let Ok(v) = std::env::var("PAYPUNK_DATA_DIR") {
            config.data_dir = v;
        }
        if let Ok(v) = std::env::var("PAYPUNK_CONFIG_DIR") {
            config.config_dir = v;
        }
        if let Ok(v) = std::env::var("PAYPUNK_ETHEREUM_RPC_URL") {
            config.ethereum_rpc_url = v;
        }
        if let Ok(v) = std::env::var("PAYPUNK_LIGHTWALLETD_HOST") {
            config.lightwalletd_host = v;
        }
        if let Ok(v) = std::env::var("PAYPUNK_ZCASH_NETWORK") {
            config.zcash_network = v;
        }
        if let Ok(v) = std::env::var("PAYPUNK_BRIDGE_SOCKET_PATH") {
            config.bridge_socket_path = v;
        }
        if let Ok(v) = std::env::var("PAYPUNK_OFFLINE_SIGNER") {
            config.offline_signer = v == "true" || v == "1";
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("config file not found at {0}")]
    NotFound(PathBuf),
    #[error("failed to read config file at {0}: {1}")]
    Io(PathBuf, std::io::Error),
    #[error("failed to parse config file at {0}: {1}")]
    Parse(PathBuf, toml::de::Error),
    #[error("could not determine config directory")]
    NoConfigDir,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn default_values() {
        let config = PaypunkConfig::default();
        assert_eq!(config.paypunkd_socket_path, "/tmp/paypunkd.sock");
        assert_eq!(config.keypunkd_socket_path, "/tmp/keypunkd.sock");
        assert_eq!(config.ethereum_rpc_url, "http://127.0.0.1:8545");
        assert_eq!(config.bridge_socket_path, "/tmp/paypunk-bridge.sock");
        assert!(!config.offline_signer);
    }

    #[test]
    fn toml_parse() {
        let toml_str = r#"
paypunkd_socket_path = "/tmp/custom.sock"
keypunkd_socket_path = "/tmp/key.sock"
data_dir = "/data/paypunk"
config_dir = "/cfg/paypunk"
ethereum_rpc_url = "http://localhost:9999"
"#;
        let config: PaypunkConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.paypunkd_socket_path, "/tmp/custom.sock");
        assert_eq!(config.keypunkd_socket_path, "/tmp/key.sock");
        assert_eq!(config.data_dir, "/data/paypunk");
        assert_eq!(config.config_dir, "/cfg/paypunk");
        assert_eq!(config.ethereum_rpc_url, "http://localhost:9999");
    }

    #[test]
    fn toml_partial_defaults() {
        let toml_str = r#"
paypunkd_socket_path = "/tmp/custom.sock"
"#;
        let config: PaypunkConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.paypunkd_socket_path, "/tmp/custom.sock");
        assert_eq!(config.keypunkd_socket_path, "/tmp/keypunkd.sock");
        assert_eq!(config.ethereum_rpc_url, "http://127.0.0.1:8545");
    }

    #[test]
    fn env_var_overrides() {
        env::set_var("PAYPUNK_SOCKET_PATH", "/env/paypunkd.sock");
        env::set_var("KEYPUNKD_SOCKET_PATH", "/env/keypunkd.sock");
        env::set_var("PAYPUNK_ETHEREUM_RPC_URL", "http://env:8545");

        let mut config = PaypunkConfig::default();
        ConfigLoader::apply_env_overrides(&mut config);

        assert_eq!(config.paypunkd_socket_path, "/env/paypunkd.sock");
        assert_eq!(config.keypunkd_socket_path, "/env/keypunkd.sock");
        assert_eq!(config.ethereum_rpc_url, "http://env:8545");

        env::remove_var("PAYPUNK_SOCKET_PATH");
        env::remove_var("KEYPUNKD_SOCKET_PATH");
        env::remove_var("PAYPUNK_ETHEREUM_RPC_URL");
    }

    #[test]
    fn missing_file_returns_defaults() {
        let config = ConfigLoader::load_or_default();
        assert_eq!(config.paypunkd_socket_path, "/tmp/paypunkd.sock");
    }

    #[test]
    fn write_and_read_default() {
        let dir = tempfile::tempdir().unwrap();
        let config_dir = dir.path().join("paypunk");
        std::fs::create_dir_all(&config_dir).unwrap();

        let config_path = config_dir.join("config.toml");
        let contents = r#"paypunkd_socket_path = "/tmp/paypunkd.sock"
keypunkd_socket_path = "/tmp/keypunkd.sock"
data_dir = "~/.local/share/paypunk/"
config_dir = "~/.config/paypunk/"
ethereum_rpc_url = "http://127.0.0.1:8545"
"#;
        std::fs::write(&config_path, contents).unwrap();

        let config = ConfigLoader::load_or_default();
        assert_eq!(config.paypunkd_socket_path, "/tmp/paypunkd.sock");
    }

    #[test]
    fn serde_roundtrip() {
        let config = PaypunkConfig::default();
        let toml_str = toml::to_string(&config).unwrap();
        let parsed: PaypunkConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(config, parsed);
    }
}
