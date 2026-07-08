# Step: Scaffold the bridge crate

Create the `bridge/` crate as a new workspace member. This crate will hold the actix-web HTTP server, Unix socket listener, and embedded HTML/JS assets for the QR bridge.

**This step is purely additive.** No existing code is modified except:
- Root `Cargo.toml`: add `bridge` to workspace members and add 3 new workspace dependencies
- No existing crate code is touched

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

[[bin]]
name = "paypunk-bridge"
path = "src/main.rs"

[dependencies]
actix-web = { workspace = true }
qrcode = { workspace = true }
base64 = { workspace = true }
clap = { workspace = true, features = ["derive"] }
tokio = { workspace = true, features = ["rt-multi-thread", "macros", "net", "signal"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
paypunk-config = { path = "../config" }

[dev-dependencies]
tempfile = { workspace = true }
```

### 4. Create `bridge/src/main.rs`

A stub binary that parses CLI args and calls into the library:

```rust
use clap::Parser;

#[derive(Parser)]
#[command(name = "paypunk-bridge")]
struct Cli {
    #[arg(long, default_value = "12345")]
    port: u16,
    #[arg(long, default_value = "/tmp/keypunkd.sock")]
    socket_path: String,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    println!("Bridge listening on {}", cli.socket_path);
    println!("Web interface: http://localhost:{}", cli.port);
    // TODO: call bridge::run() in step 2
}
```

### 5. Create `bridge/src/lib.rs`

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

### 6. Create placeholder asset files

- `bridge/src/bridge.html` — `<html><body><p>Bridge</p></body></html>`
- `bridge/src/jsqr.js` — empty file (downloaded from jsQR GitHub in step 2)

## Acceptance criteria

- [ ] `cargo build` succeeds from workspace root
- [ ] `cargo build -p paypunk-bridge` succeeds
- [ ] `cargo run -p paypunk-bridge -- --help` shows `--port` and `--socket-path` flags
- [ ] `cargo run -p paypunk-bridge` prints startup message and exits cleanly

## Context

- The bridge sits between paypunkd and keypunkd, replacing keypunkd's Unix socket
- Socket path defaults to `/tmp/keypunkd.sock` so paypunkd connects transparently
- Port defaults to `12345` for the web interface
- The bridge is a standalone binary crate, NOT part of the CLI crate
- `bridge/src/bridge.html` and `bridge/src/jsqr.js` are embedded at compile time via `include_str!` / `include_bytes!` — they must live in `bridge/src/` (relative to the Rust source files that reference them)

## Implementation instructions for agent

1. Add `actix-web`, `qrcode` (svg), and `base64` to `[workspace.dependencies]` in root `Cargo.toml`
2. Add `"bridge"` to workspace `members` list in root `Cargo.toml`
3. Create `bridge/Cargo.toml` with content above
4. Create `bridge/src/main.rs` with stub above
5. Create `bridge/src/lib.rs` with stub above
6. Create placeholder `bridge/src/bridge.html`
7. Create empty `bridge/src/jsqr.js`
8. Run `cargo build` to verify it compiles
9. Run `cargo fmt --all`
10. Move this step file to `project/done/01_step.md`
11. Commit with message: `feat: scaffold bridge crate`
