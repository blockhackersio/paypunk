# Goal 1: Config struct with hardcoded defaults

## Context

Currently both `keypunkd` and `paypunkd` accept socket paths and data directories via CLI args with hardcoded defaults in their `Args` structs:
- `paypunkd/src/main.rs:16-23`: `--socket-path` defaults to `/tmp/paypunkd.sock`, `--keypunkd-socket` defaults to `/tmp/keypunkd.sock`, `--rpc-url` defaults to `http://127.0.0.1:8545`
- `keypunkd/src/main.rs:21-27`: `--socket-path` defaults to `/tmp/keypunkd.sock`, `--data-dir` defaults to `/tmp/paypunk/data`
- `cli/src/main.rs:11-12`: `--socket-path` defaults to `/tmp/paypunkd.sock`

We need a single `Config` source of truth so that:
1. Daemons and CLI use consistent hardcoded paths without CLI-arg duplication
2. Future user-config overrides can be swapped in via a trait without changing consumers
3. The orchestration layer (Goal 4) can read config to know where to find sockets

## Implementation plan

### 1. Create `paypunkd/src/config.rs`

Define a `ConfigSource` trait and a `HardcodedConfig` implementation:

```rust
/// Source of configuration values — allows swapping hardcoded defaults
/// for user-config-file values later without changing consumers.
pub trait ConfigSource {
    fn paypunkd_socket_path(&self) -> &str;
    fn keypunkd_socket_path(&self) -> &str;
    fn data_dir(&self) -> &Path;
    fn config_dir(&self) -> &Path;
    fn rpc_url(&self) -> &str;
}

/// Hardcoded default configuration.
///
/// All values are compile-time constants. Replace the implementation
/// of ConfigSource to read from ~/.config/paypunk/config.toml later.
pub struct HardcodedConfig;
```

Hardcoded values should use:
- Socket paths: `/tmp/paypunkd.sock`, `/tmp/keypunkd.sock`
- Data dir: `~/.local/share/paypunk/` (expand via `dirs::data_dir()` or hardcode to `~/.local/share/paypunk/`)
- Config dir: `~/.config/paypunk/`
- RPC URL: `http://127.0.0.1:8545`

### 2. Expose `config` module from `paypunkd/src/lib.rs`

Add `pub mod config;` to `paypunkd/src/lib.rs`.

### 3. Update `paypunkd/src/main.rs`

Replace the `Args` struct's hardcoded defaults with usage of `HardcodedConfig`. Keep the CLI args but change their defaults to read from config. The daemon should construct `HardcodedConfig` at startup and pass config values through.

### 4. Update `keypunkd/src/main.rs`

Similarly, use the config (or at minimum, have keypunkd also accept config). Since `ConfigSource` lives in `paypunkd`, keypunkd can either:
- Duplicate the pattern with its own simple config
- Or accept config values passed in from the orchestration layer

For now, keep keypunkd's Args but update defaults to match the config.

### 5. Add `dirs` dependency

Add `dirs = "6"` to workspace `Cargo.toml` for XDG directory resolution, or use `std::env::var("HOME")` to construct paths manually to avoid adding a dependency.

## Files to modify

- `paypunkd/src/lib.rs` — add `pub mod config;`
- `paypunkd/src/config.rs` — **new file**: `ConfigSource` trait + `HardcodedConfig`
- `paypunkd/src/main.rs` — use config for defaults
- `keypunkd/src/main.rs` — align defaults with config
- `cli/src/main.rs` — align defaults with config

## Tests

### Unit test: `config_returns_expected_defaults`

```rust
#[test]
fn test_hardcoded_config_defaults() {
    let config = HardcodedConfig;
    assert!(config.paypunkd_socket_path().contains("paypunkd.sock"));
    assert!(config.keypunkd_socket_path().contains("keypunkd.sock"));
    assert!(config.data_dir().to_string_lossy().contains("paypunk"));
}
```

### Unit test: `config_source_trait_is_implemented`

```rust
#[test]
fn test_config_source_trait() {
    let config: &dyn ConfigSource = &HardcodedConfig;
    assert!(!config.paypunkd_socket_path().is_empty());
}
```

## Acceptance criteria

- `Config` struct exists with getters for: `paypunkd_socket_path`, `keypunkd_socket_path`, `data_dir`, `config_dir`
- All defaults are hardcoded (e.g., `/tmp/paypunkd.sock`, `~/.local/share/paypunk/`)
- A `ConfigSource` trait abstracts where values come from, with a `HardcodedConfig` implementation
- Unit test: `Config` returns expected default values
- Unit test: `HardcodedConfig` implements `ConfigSource` correctly
