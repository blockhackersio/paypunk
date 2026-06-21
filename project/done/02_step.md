# Step 2: Update CLI to call daemon libraries in-process

## Context

The CLI's `Commands::Keypunkd` and `Commands::Paypunkd` handlers currently spawn child processes via `Command::new("keypunkd")` / `Command::new("paypunkd")`. Since those standalone binaries no longer exist (Step 1), these handlers must instead call the daemon library `run()` functions directly in-process.

## Changes

### 1. `cli/Cargo.toml`

Add `keypunkd` and `paypunkd` as dependencies:

```toml
[dependencies]
# ... existing deps ...
keypunkd.workspace = true
paypunkd.workspace = true
```

### 2. `cli/src/main.rs`

Replace the `Commands::Keypunkd` handler (currently lines 124-157):

**Before:**
```rust
Some(Commands::Keypunkd { socket_path, data_dir }) => {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let config = ConfigLoader::load_or_default();
        let socket = socket_path.unwrap_or(config.keypunkd_socket_path);
        let dir = data_dir.unwrap_or(config.data_dir);

        let mut child = Command::new("keypunkd")
            .arg("--socket-path").arg(&socket)
            .arg("--data-dir").arg(&dir)
            .spawn()
            .map_err(|e| format!("Failed to start keypunkd: {e}"))?;

        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_clone = shutdown.clone();
        tokio::spawn(async move {
            tokio::signal::ctrl_c().await.ok();
            shutdown_clone.store(true, Ordering::SeqCst);
        });

        while !shutdown.load(Ordering::SeqCst) {
            if let Ok(Some(_)) = child.try_wait() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        let _ = child.kill();
        let _ = child.wait();
        Ok(())
    })
}
```

**After:**
```rust
Some(Commands::Keypunkd { socket_path, data_dir }) => {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let config = ConfigLoader::load_or_default();
        let socket = socket_path.unwrap_or(config.keypunkd_socket_path);
        let dir = data_dir.unwrap_or(config.data_dir);

        keypunkd::run::run(keypunkd::run::Config {
            socket_path: socket,
            data_dir: dir,
        })
        .await
    })
}
```

Replace the `Commands::Paypunkd` handler (currently lines 158-201):

**Before:**
```rust
Some(Commands::Paypunkd { socket_path, keypunkd_socket, rpc_url, data_dir }) => {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let config = ConfigLoader::load_or_default();
        let socket = socket_path.unwrap_or(config.paypunkd_socket_path);
        let ks = keypunkd_socket.unwrap_or(config.keypunkd_socket_path);
        let url = rpc_url.unwrap_or(config.rpc_url);
        let dir = data_dir.unwrap_or(config.data_dir);

        let mut child = Command::new("paypunkd")
            .arg("--socket-path").arg(&socket)
            .arg("--keypunkd-socket").arg(&ks)
            .arg("--rpc-url").arg(&url)
            .arg("--data-dir").arg(&dir)
            .spawn()
            .map_err(|e| format!("Failed to start paypunkd: {e}"))?;

        // ... Ctrl+C handler + polling loop ...

        let _ = child.kill();
        let _ = child.wait();
        Ok(())
    })
}
```

**After:**
```rust
Some(Commands::Paypunkd { socket_path, keypunkd_socket, rpc_url, data_dir }) => {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let config = ConfigLoader::load_or_default();
        let socket = socket_path.unwrap_or(config.paypunkd_socket_path);
        let ks = keypunkd_socket.unwrap_or(config.keypunkd_socket_path);
        let url = rpc_url.unwrap_or(config.rpc_url);
        let dir = data_dir.unwrap_or(config.data_dir);

        paypunkd::run::run(paypunkd::run::Config {
            socket_path: socket,
            keypunkd_socket: ks,
            rpc_url: url,
            data_dir: dir,
        })
        .await
    })
}
```

### 3. Clean up unused imports

Remove these imports from `cli/src/main.rs` that are no longer needed:
- `use std::process::Command;`
- `use std::time::Duration;`
- `use std::sync::Arc;`
- `use std::sync::atomic::{AtomicBool, Ordering};`

(Keep `Arc` and `AtomicBool` if they're used elsewhere — check.)

## Verification

- `cargo build` succeeds
- `cargo test` passes
- `paypunk keypunkd --socket-path /tmp/test-ks.sock --data-dir /tmp/test-kd` starts keypunkd in-process (verify by checking the socket file appears)
- `paypunk paypunkd --socket-path /tmp/test-ps.sock --keypunkd-socket /tmp/test-ks.sock --data-dir /tmp/test-pd` starts paypunkd in-process (requires keypunkd already running on the keypunkd socket)

## Acceptance criteria

- `paypunk keypunkd` runs keypunkd in the same process (no child process spawned)
- `paypunk paypunkd` runs paypunkd in the same process (no child process spawned)
- All existing CLI subcommands still work
- All tests pass
