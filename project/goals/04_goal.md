# Goal 4: Orchestration — paypunk launches daemons as child processes

## Context

Currently the `paypunk` CLI binary (`cli/src/main.rs`) requires both `keypunkd` and `paypunkd` to already be running. The user must start them manually in separate terminals. This is a poor developer and user experience.

The goal is to make `paypunk` (no args) or `paypunk tui` automatically:
1. Start `keypunkd` as a child process
2. Start `paypunkd` as a child process
3. Wait for both Unix sockets to be ready
4. Launch the TUI
5. Clean up child processes on exit

The existing `cli/src/main.rs:85-88` shows the current TUI launch path:
```rust
None | Some(Commands::Tui) => {
    let rt = tokio::runtime::Runtime::new()?;
    Ok(rt.block_on(paypunk_tui::run_tui(Some(cli.socket_path)))?)
}
```

The `tui/src/lib.rs:24-36` shows how the TUI connects to paypunkd:
```rust
pub async fn run_tui(socket_path: Option<String>) -> io::Result<()> {
    let api: Box<dyn WalletApi> = if let Some(path) = socket_path {
        match RealWalletApi::connect(&path).await {
            Ok(real) => Box::new(real),
            Err(e) => { /* fallback to mock */ }
        }
    } else {
        Box::new(MockWalletApi::new())
    };
```

Hardcoded socket paths (from Goal 1) will be used:
- `keypunkd`: `/tmp/keypunkd.sock`
- `paypunkd`: `/tmp/paypunkd.sock`

## Implementation plan

### 1. Add daemon spawning to `cli/src/main.rs`

Modify the TUI launch path to spawn daemon processes:

```rust
use std::process::{Child, Command};
use std::time::Duration;

struct DaemonProcess {
    keypunkd: Child,
    paypunkd: Child,
}

async fn spawn_daemons() -> Result<DaemonProcess, Box<dyn std::error::Error>> {
    // Start keypunkd
    let keypunkd = Command::new("keypunkd")
        .spawn()
        .map_err(|e| format!("Failed to start keypunkd: {e}"))?;

    // Start paypunkd
    let paypunkd = Command::new("paypunkd")
        .spawn()
        .map_err(|e| format!("Failed to start paypunkd: {e}"))?;

    // Wait for sockets to be ready
    wait_for_socket("/tmp/keypunkd.sock", Duration::from_secs(10)).await?;
    wait_for_socket("/tmp/paypunkd.sock", Duration::from_secs(10)).await?;

    Ok(DaemonProcess { keypunkd, paypunkd })
}

async fn wait_for_socket(path: &str, timeout: Duration) -> Result<(), String> {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if std::path::Path::new(path).exists() {
            // Try connecting to verify it's accepting connections
            if let Ok(_stream) = tokio::net::UnixStream::connect(path).await {
                return Ok(());
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(format!("socket {path} did not become ready within timeout"))
}
```

### 2. Modify `cli/src/main.rs` TUI launch

The `None | Some(Commands::Tui)` arm should:

```rust
None | Some(Commands::Tui) => {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let daemons = spawn_daemons().await?;
        let result = paypunk_tui::run_tui(Some(config.paypunkd_socket_path())).await;
        // Cleanup: kill daemons
        let _ = daemons.keypunkd.kill();
        let _ = daemons.paypunkd.kill();
        let _ = daemons.keypunkd.wait();
        let _ = daemons.paypunkd.wait();
        result.map_err(|e| e.into())
    })
}
```

### 3. Handle Ctrl+C gracefully

Use `tokio::signal::ctrl_c()` to ensure daemons are killed if the user presses Ctrl+C. This can be done by spawning a signal handler task that sets a shutdown flag, or by using the TUI's existing Ctrl+C handling (`tui/src/lib.rs:79-81`).

The TUI already handles Ctrl+C in its event loop:
```rust
if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
    app.should_quit = true;
}
```

After TUI exits, the cleanup code runs. But if the TUI is killed abruptly (SIGTERM), the cleanup in the `rt.block_on` might not run. Consider:
- Registering a signal handler before spawning daemons
- Using a `Drop` impl on a wrapper that kills children

### 4. Use config from Goal 1

Use `paypunkd::config::HardcodedConfig` to get socket paths instead of hardcoding them in the CLI.

## Files to modify

- `cli/src/main.rs` — add daemon spawning, TUI orchestration, cleanup
- `cli/Cargo.toml` — may need to add `tokio` dependency (check if already present)

## Tests

### Unit test: `wait_for_socket_timeout`

```rust
#[tokio::test]
async fn test_wait_for_socket_timeout() {
    let result = wait_for_socket("/tmp/nonexistent-test-socket-12345.sock", Duration::from_millis(50)).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("did not become ready"));
}
```

### Integration test: `spawn_daemons_starts_processes`

This test requires the daemon binaries to be built first. It should:
1. Call `spawn_daemons()`
2. Verify both sockets exist
3. Connect to paypunkd and send a simple request (e.g., GetEncryptionKey)
4. Kill daemons
5. Verify sockets are cleaned up

```rust
#[tokio::test]
async fn test_orchestration_launches_daemons() {
    let daemons = spawn_daemons().await.unwrap();
    
    // Verify sockets exist
    assert!(Path::new("/tmp/keypunkd.sock").exists());
    assert!(Path::new("/tmp/paypunkd.sock").exists());
    
    // Can connect to paypunkd
    let client = paypunk_api::Client::connect("/tmp/paypunkd.sock").await.unwrap();
    
    // Cleanup
    daemons.keypunkd.kill().unwrap();
    daemons.paypunkd.kill().unwrap();
    daemons.keypunkd.wait().unwrap();
    daemons.paypunkd.wait().unwrap();
}
```

## Acceptance criteria

- Running `paypunk` (no args) spawns both daemon processes
- Both daemons are reachable via their hardcoded socket paths
- TUI launches and can communicate with paypunkd
- On Ctrl+C or TUI exit, both daemon processes are killed
- Unit test: child process spawning logic works (test with mock daemons)
- Integration test: full orchestration with actual daemon binaries starts and stops cleanly
