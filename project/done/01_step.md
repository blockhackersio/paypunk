# Step: Scaffold the bridge crate

Create the `bridge/` crate as a new workspace member. This crate will hold the actix-web HTTP server, Unix socket listener, and embedded HTML/JS assets for the QR bridge.

**Existing code changes are moderate.** The root `Cargo.toml` is updated (workspace member + 3 new deps), `cli/Cargo.toml` gains the bridge dependency, and `cli/src/main.rs` gets a new `Bridge` subcommand that delegates to the bridge library. The bridge itself is a lib-only crate — no standalone binary.

## Tasks

### 1. Add workspace dependencies to root `Cargo.toml`

Add these entries under `[workspace.dependencies]`:

```toml
actix-web = "4"
qrcode = { version = "0.14", features = ["svg"] }
base64 = "0.22"
```

### 2. Add `"bridge"` to workspace members list

```toml
members = [
    ...
    "bridge",
]
```

### 3. Create `bridge/Cargo.toml`

```toml
[package]
name = "paypunk-bridge"
version = "0.1.0"
edition = "2021"

[dependencies]
actix-web = { workspace = true }
qrcode = { workspace = true }
base64 = { workspace = true }
tokio = { workspace = true, features = ["rt-multi-thread", "macros", "net", "signal"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
paypunk-config = { path = "../config" }

[dev-dependencies]
tempfile = { workspace = true }
```

### 4. Add `paypunk-bridge` dependency to `cli/Cargo.toml`

Add this line under `[dependencies]` in `cli/Cargo.toml`:

```toml
paypunk-bridge = { path = "../bridge" }
```

### 5. Add `Bridge` variant to the CLI `Commands` enum

In `cli/src/main.rs`, add a new variant to the `Commands` enum:

```rust
/// Run the QR bridge web server
Bridge {
    #[arg(long, default_value = "12345")]
    port: u16,
    #[arg(long, default_value = "/tmp/keypunkd.sock")]
    socket_path: String,
},
```

### 6. Add handler in the CLI main match block

In the `main()` function, before the final catch-all `Some(command)` block that connects to paypunkd, add a dedicated arm for the `Bridge` command:

```rust
Some(Commands::Bridge { port, socket_path }) => {
    let config = paypunk_bridge::BridgeConfig {
        port,
        socket_path,
    };
    paypunk_bridge::run(config).await?;
    Ok(())
}
```

Place this arm **before** the catch-all block that calls `ensure_daemons`, since the bridge runs standalone (it replaces keypunkd's socket, so paypunkd should connect through it).

### 7. Create `bridge/src/lib.rs`

Library root with a stub `run` function and config type:

```rust
pub struct BridgeConfig {
    pub port: u16,
    pub socket_path: String,
}

pub async fn run(config: BridgeConfig) -> Result<(), Box<dyn std::error::Error>> {
    // Stub — will be implemented in step 2
    Ok(())
}
```

### 8. Create placeholder asset files

- `bridge/src/bridge.html` — `<html><body><p>Bridge</p></body></html>`
- `bridge/src/jsqr.js` — empty file (downloaded from jsQR GitHub in step 2)

## Acceptance criteria

- [ ] `cargo build` succeeds from workspace root
- [ ] `cargo build -p paypunk-bridge` succeeds
- [ ] `paypunk bridge --help` shows `--port` and `--socket-path` flags
- [ ] `paypunk bridge` prints startup message and exits cleanly

## Context

- The bridge sits between paypunkd and keypunkd, replacing keypunkd's Unix socket
- Socket path defaults to `/tmp/keypunkd.sock` so paypunkd connects transparently
- Port defaults to `12345` for the web interface
- The bridge is a lib-only crate; it is invoked via the CLI's `paypunk bridge` subcommand
- `bridge/src/bridge.html` and `bridge/src/jsqr.js` are embedded at compile time via `include_str!` / `include_bytes!` — they must live in `bridge/src/` (relative to the Rust source files that reference them)

## Implementation instructions for agent

1. Add `actix-web`, `qrcode` (svg), and `base64` to `[workspace.dependencies]` in root `Cargo.toml`
2. Add `"bridge"` to workspace `members` list in root `Cargo.toml`
3. Create `bridge/Cargo.toml` with content above (no `[[bin]]` section — lib only)
4. Add `paypunk-bridge = { path = "../bridge" }` to `cli/Cargo.toml` dependencies
5. Add `Bridge` variant to the `Commands` enum in `cli/src/main.rs`
6. Add the `Commands::Bridge` handler in `cli/src/main.rs` (before the catch-all daemon block)
7. Create `bridge/src/lib.rs` with stub above
8. Create placeholder `bridge/src/bridge.html`
9. Create empty `bridge/src/jsqr.js`
10. Run `cargo build` to verify it compiles
11. Run `cargo fmt --all`
12. Move this step file to `project/done/01_step.md`
13. Commit with message: `feat: scaffold bridge crate and integrate into CLI`
