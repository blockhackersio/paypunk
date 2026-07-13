# AGENTS.md

## Build

```bash
cargo build
```

## Test

```bash
cargo test
```

## Format check (CI-enforced)

```bash
cargo fmt --all --check
```

## Lint (recommended, not CI-gated)

```bash
cargo clippy --all-targets
```

## CI

CI runs on push/PR to `master`:
1. `cargo fmt --all --check`
2. `cargo test`

## Workspace layout

- 16 workspace members in root `Cargo.toml`
- `signer/src-tauri` is excluded (separate Tauri v2 build)
- `vendor/ratatui-widgets` is patched via `[patch.crates-io]`
- Rust stable, edition 2021, resolver 2

## Key crates

- `types` — domain types + `Protocol`/`SignerProtocol` traits
- `ipc` — tactix actor IPC over Unix sockets
- `keypunkd` — key daemon (seed, signing)
- `paypunkd` — app daemon (wallet DB, protocol orchestration)
- `api` — public client library
- `cli` — `paypunk` binary (CLI + TUI + daemon launcher)
- `protocols/{zcash,ethereum}` — chain implementations

## Conventions

- No comments unless explicitly asked
- Dependencies via `workspace = true`
- Serialization: postcard (IPC), toml (config)
- Actor framework: tactix
- License: AGPL-3.0-only
