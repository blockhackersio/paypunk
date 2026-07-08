# Step: Create the ping CLI crate

Create the `ping/` crate — a binary that connects to a Unix socket, performs the full IPC handshake, sends a `ping` IPC message, waits for a `pong` response, and prints the result.

**This step is purely additive.** Only the root `Cargo.toml` (add workspace member) and the new `ping/` directory are touched.

## Tasks

### 1. Add `"ping"` to workspace members in root `Cargo.toml`

```toml
members = [
    ...
    "ping",
]
```

### 2. Create `ping/Cargo.toml`

```toml
[package]
name = "paypunk-ping"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "paypunk-ping"
path = "src/main.rs"

[dependencies]
paypunk-ipc = { path = "../ipc" }
tactix = { workspace = true }
tokio = { workspace = true, features = ["rt", "macros"] }
clap = { workspace = true, features = ["derive"] }
```

### 3. Create `ping/src/main.rs`

A CLI binary that:
1. Parses `--socket-path` argument (default `/tmp/keypunkd.sock`)
2. Connects to the Unix socket via `IpcSender::connect()`
3. Sends an `IpcMessage` with payload `b"ping"` via `addr.ask()`
4. Matches the result:
   - `Ok(bytes)` where `bytes == b"pong"` → prints `✅ Pong received` and exits 0
   - `Ok(other)` → prints `❌ Unexpected response: <hex/string>` and exits 1
   - `Err(e)` → prints `❌ Error: {e}` and exits 1

```rust
use clap::Parser;
use paypunk_ipc::{IpcMessage, IpcSender};

#[derive(Parser)]
#[command(name = "paypunk-ping")]
struct Cli {
    #[arg(long, default_value = "/tmp/keypunkd.sock")]
    socket_path: String,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    println!("Connecting to {}...", cli.socket_path);

    let addr = match IpcSender::connect(&cli.socket_path).await {
        Ok(addr) => addr,
        Err(e) => {
            eprintln!("❌ Failed to connect: {e}");
            std::process::exit(1);
        }
    };

    println!("Sending ping...");

    let result = addr.ask(IpcMessage::new(b"ping".to_vec())).await;

    match result {
        Ok(bytes) => {
            if bytes == b"pong" {
                println!("✅ Pong received");
            } else {
                eprintln!(
                    "❌ Unexpected response: {}",
                    String::from_utf8_lossy(&bytes)
                );
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("❌ Error: {e}");
            std::process::exit(1);
        }
    }
}
```

## Acceptance criteria

- [ ] `cargo build` succeeds from workspace root
- [ ] `cargo build -p paypunk-ping` succeeds
- [ ] `cargo run -p paypunk-ping -- --help` shows the `--socket-path` flag
- [ ] Running `cargo run -p paypunk-ping` against an active bridge socket performs the IPC handshake, sends ping, and prints the result

## Context

- The ping CLI uses `IpcSender::connect()` which performs the full IPC handshake (GetPublicKey → PublicKey, RegisterClient → Ack) and derives the HMAC key
- It then sends an application frame (`MSG_APPLICATION`) containing payload `b"ping"` with a valid MAC
- The response is expected to be `b"pong"` (the payload after stripping the `0x00` success status byte)
- For the ping CLI to receive a pong, something must respond via the bridge's HTTP API:
  1. A test agent reads the pending bytes from `GET /pending-bytes`
  2. Feeds them to `PongHandler::handle()` from the `paypunk-pong` crate
  3. POSTs the result to `/response`
- The bridge writes the response bytes back to the Unix socket, and `IpcSender` decodes them
- No existing crate code outside `ping/` is modified

## Implementation instructions for agent

1. Add `"ping"` to workspace members in root `Cargo.toml`
2. Create `ping/Cargo.toml`
3. Create `ping/src/main.rs`
4. Run `cargo build` to verify it compiles
5. Run `cargo fmt --all`
6. Move this step file to `project/done/04_step.md`
7. Commit with message: `feat: create ping CLI for IPC roundtrip testing`
