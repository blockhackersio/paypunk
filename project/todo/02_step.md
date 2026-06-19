# Step 02: Wire shared config into all binaries

## Goal

Replace hardcoded socket paths, data dirs, and RPC URLs in `paypunkd`, `keypunkd`, `cli`, and `tui` with values from `paypunk-config`. Remove `db_password()` from `ConfigSource` trait (password no longer lives in config).

## Tasks

1. Update `paypunkd/src/config.rs`:
   - Remove `db_password()` from `ConfigSource` trait
   - Add a `TomlConfig` wrapper that implements `ConfigSource` using `PaypunkConfig`
   - Keep `HardcodedConfig` as fallback but without `db_password()`

2. Update `paypunkd/src/main.rs`:
   - Load config via `ConfigLoader::load_or_default()`
   - Use `config.paypunkd_socket_path()` instead of hardcoded or CLI fallback
   - Use `config.keypunkd_socket_path()` for keypunkd connection
   - Use `config.data_dir()` for DB path
   - Use `config.rpc_url()` for ETH RPC
   - **Do not** pass any DB password — DB will be locked at startup

3. Update `keypunkd/src/main.rs`:
   - Load config via `ConfigLoader::load_or_default()`
   - Use `config.keypunkd_socket_path()` for socket
   - Use `config.data_dir()` for seed store path
   - Remove its own independent defaults

4. Update `cli/src/main.rs`:
   - Load config for default socket path
   - Use `config.paypunkd_socket_path()` as `--socket-path` default

5. Update `tui/src/lib.rs`:
   - Accept config (or socket path from config) instead of raw `Option<String>`

6. Update `paypunkd/src/database/db.rs`:
   - `Database::open()` should not require a password parameter
   - The DB file may or may not exist; if it exists, it stays encrypted until `unlock()` is called
   - Add `wallet_exists()` method that checks if the encrypted DB file exists

## Acceptance Criteria

- [ ] `cargo check` succeeds for the whole workspace
- [ ] `cargo test` passes for all crates
- [ ] `paypunkd` starts and reads config from TOML file
- [ ] `keypunkd` starts and reads config from TOML file
- [ ] CLI defaults come from config
- [ ] `PAYPUNK_SOCKET_PATH` env var overrides the config file
- [ ] No hardcoded `/tmp/` socket paths remain in binary source
- [ ] `ConfigSource::db_password()` no longer exists
- [ ] Code is committed with message: "feat: wire shared config into all binaries, remove hardcoded paths"
