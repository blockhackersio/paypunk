# Step 01: Create `paypunk-config` shared configuration crate

## Goal

Create a new workspace crate that provides shared configuration loading from a TOML file with environment variable overrides. This eliminates hardcoded socket paths, data directories, and RPC URLs.

## Tasks

1. Create `config/Cargo.toml` with `serde`, `toml`, `thiserror`, `dirs` dependencies
2. Create `config/src/lib.rs` with:
   - `PaypunkConfig` struct (serde Deserialize):
     - `paypunkd_socket_path: String` (default: `/tmp/paypunkd.sock`)
     - `keypunkd_socket_path: String` (default: `/tmp/keypunkd.sock`)
     - `data_dir: String` (default: `~/.local/share/paypunk/`)
     - `config_dir: String` (default: `~/.config/paypunk/`)
     - `rpc_url: String` (default: `http://127.0.0.1:8545`)
   - `ConfigLoader` struct with:
     - `load()` — reads `~/.config/paypunk/config.toml`, merges with env var overrides (`PAYPUNK_SOCKET_PATH`, `KEYPUNKD_SOCKET_PATH`, `PAYPUNK_DATA_DIR`, `PAYPUNK_CONFIG_DIR`, `PAYPUNK_RPC_URL`)
     - `load_or_default()` — returns default config if file doesn't exist
     - `write_default()` — writes a commented default config file
3. Register `config` in workspace `Cargo.toml` members
4. Add tests for:
   - Default config values
   - TOML parsing
   - Env var overrides
   - Missing file falls back to defaults

## Acceptance Criteria

- [ ] `cargo check -p paypunk-config` succeeds
- [ ] `cargo test -p paypunk-config` passes all tests
- [ ] `PaypunkConfig` can be serialized/deserialized from TOML
- [ ] Environment variables override TOML file values
- [ ] Missing config file returns defaults without error
- [ ] `write_default()` creates a valid TOML file
- [ ] Code is committed with message: "feat: add paypunk-config crate for shared TOML configuration"
