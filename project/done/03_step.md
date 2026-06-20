# Step 3: Add orchestrator mode for `paypunk` (no args)

## Context

When the user runs `paypunk` with no subcommand, it should:
1. Spawn `paypunk keypunkd` as a child process (using `env::current_exe()`)
2. Spawn `paypunk paypunkd` as a child process (using `env::current_exe()`)
3. Wait for both daemon socket files to appear
4. Launch the TUI
5. On Ctrl+C or TUI exit, kill the child daemon processes

The current `None` handler just launches the TUI directly. We need to replace it with the orchestrator flow.

## Changes

### `cli/src/main.rs`

Replace the `None` (and `Some(Commands::Tui)`) handler:

**Current code (lines 109-123):**
```rust
match cli.command {
    None | Some(Commands::Tui) => {
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async {
            let shutdown = Arc::new(AtomicBool::new(false));
            let shutdown_clone = shutdown.clone();
            tokio::spawn(async move {
                tokio::signal::ctrl_c().await.ok();
                shutdown_clone.store(true, Ordering::SeqCst);
            });

            let result = paypunk_tui::run_tui(&socket_path, Some(shutdown)).await;
            result.map_err(|e| e.into())
        })
    }
```

**New code:**
```rust
None | Some(Commands::Tui) => {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let config = ConfigLoader::load_or_default();
        let exe = std::env::current_exe()
            .map_err(|e| format!("Failed to get current exe path: {e}"))?;
        let paypunkd_socket = cli.socket_path.unwrap_or(config.paypunkd_socket_path);
        let keypunkd_socket = config.keypunkd_socket_path.clone();
        let data_dir = config.data_dir.clone();
        let rpc_url = config.rpc_url.clone();

        // Spawn keypunkd child
        let mut keypunkd_child = std::process::Command::new(&exe)
            .arg("keypunkd")
            .arg("--socket-path")
            .arg(&keypunkd_socket)
            .arg("--data-dir")
            .arg(&data_dir)
            .spawn()
            .map_err(|e| format!("Failed to spawn keypunkd: {e}"))?;

        // Spawn paypunkd child
        let mut paypunkd_child = std::process::Command::new(&exe)
            .arg("paypunkd")
            .arg("--socket-path")
            .arg(&paypunkd_socket)
            .arg("--keypunkd-socket")
            .arg(&keypunkd_socket)
            .arg("--rpc-url")
            .arg(&rpc_url)
            .arg("--data-dir")
            .arg(&data_dir)
            .spawn()
            .map_err(|e| format!("Failed to spawn paypunkd: {e}"))?;

        // Wait for both socket files to appear (timeout: 30 seconds)
        let wait_result = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            wait_for_sockets(&[&keypunkd_socket, &paypunkd_socket]),
        )
        .await;

        if wait_result.is_err() {
            // Timeout — kill children and return error
            let _ = keypunkd_child.kill();
            let _ = paypunkd_child.kill();
            let _ = keypunkd_child.wait();
            let _ = paypunkd_child.wait();
            return Err("Timed out waiting for daemon sockets to appear".into());
        }

        // Run TUI
        let shutdown = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let shutdown_clone = shutdown.clone();
        tokio::spawn(async move {
            tokio::signal::ctrl_c().await.ok();
            shutdown_clone.store(true, std::sync::atomic::Ordering::SeqCst);
        });

        let tui_result = paypunk_tui::run_tui(&paypunkd_socket, Some(shutdown)).await;

        // Shutdown: kill children
        let _ = keypunkd_child.kill();
        let _ = paypunkd_child.kill();
        let _ = keypunkd_child.wait();
        let _ = paypunkd_child.wait();

        tui_result.map_err(|e| e.into())
    })
}
```

### Add helper function

Add this function to `cli/src/main.rs` (before `main()` or after it):

```rust
async fn wait_for_sockets(paths: &[&str]) {
    use std::path::Path;
    loop {
        let all_exist = paths.iter().all(|p| Path::new(p).exists());
        if all_exist {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}
```

### Restore necessary imports

Re-add the imports that were removed in Step 2 (if they were removed):
- `use std::sync::Arc;`
- `use std::sync::atomic::{AtomicBool, Ordering};` (or use full paths as shown above)

Also add:
- `use std::process::Command;` — actually no, use the full path `std::process::Command::new(&exe)` as shown. This avoids needing the import.

Actually, looking at the code above, I used `std::process::Command` with full paths, so no import is needed. Same for `Arc`, `AtomicBool` — I used full paths. Let's keep it clean.

## Verification

- `cargo build` succeeds
- `cargo test` passes
- Run `paypunk` (no args) — it should start keypunkd, paypunkd, then the TUI
- Verify Ctrl+C kills both daemon child processes
- Verify `paypunk tui` still works as before (same as no args)

## Acceptance criteria

- `paypunk` (no args) spawns `paypunk keypunkd` and `paypunk paypunkd` as child processes using `env::current_exe()`
- Waits for daemon socket files to appear (with 30s timeout)
- Launches TUI after daemons are ready
- Ctrl+C or TUI exit kills both daemon children
- `paypunk tui` behaves identically to `paypunk` (no args)
- All existing tests pass
