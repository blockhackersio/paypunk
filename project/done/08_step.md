# Step 8: CLI config switch — `--signer` flag

## Goal

Add a `--signer` CLI flag. When set: skip spawning `keypunkd`, spawn the bridge
instead, connect paypunkd to the bridge socket, pass `signer_mode: true` to
`run_tui()`.

## Files to change

### 1. `cli/src/main.rs` — `ensure_daemons` function

Update `ensure_daemons` to accept a `signer_mode: bool` parameter. When
`signer_mode` is true:
- Skip spawning `keypunkd`
- Spawn the bridge instead
- Connect paypunkd to the bridge socket (instead of the keypunkd socket)

The function signature changes from:

```rust
async fn ensure_daemons(paypunkd_socket: &str, keypunkd_socket: &str) -> Result<DaemonGuard, ...>
```

to:

```rust
async fn ensure_daemons(
    paypunkd_socket: &str,
    keypunkd_socket: &str,
    bridge_socket: &str,
    signer_mode: bool,
) -> Result<DaemonGuard, ...>
```

In the body, branch on `signer_mode`:

```rust
if signer_mode {
    // Spawn bridge instead of keypunkd
    let bridge = Command::new(&exe)
        .arg("bridge")
        .arg("--socket")
        .arg(bridge_socket)
        .spawn()
        .context("failed to spawn bridge")?;
    guard.add_child(bridge);
    // Wait for bridge socket to appear
    wait_for_socket(bridge_socket, Duration::from_secs(30)).await?;
} else {
    // Existing keypunkd spawn logic
    let keypunkd = Command::new(&exe)
        .arg("keypunkd")
        .arg("--socket")
        .arg(keypunkd_socket)
        .spawn()
        .context("failed to spawn keypunkd")?;
    guard.add_child(keypunkd);
    wait_for_socket(keypunkd_socket, Duration::from_secs(30)).await?;
}

// Paypunkd always spawns
let paypunkd = Command::new(&exe)
    .arg("paypunkd")
    .arg("--socket")
    .arg(paypunkd_socket)
    .arg(if signer_mode { "--keypunkd-socket" } else { "--keypunkd-socket" })
    .arg(if signer_mode { bridge_socket } else { keypunkd_socket })
    .spawn()
    .context("failed to spawn paypunkd")?;
guard.add_child(paypunkd);
wait_for_socket(paypunkd_socket, Duration::from_secs(30)).await?;
```

### 2. `cli/src/main.rs` — Add `--signer` argument

In the CLI argument parsing, add the `--signer` flag. If using `clap`:

```rust
#[derive(Parser)]
struct Cli {
    // ... existing args ...
    #[arg(long, default_value_t = false)]
    signer: bool,
}
```

If using manual argument parsing, add the equivalent.

### 3. `cli/src/main.rs` — Default (no subcommand) flow

Update the default flow (the code that runs when no subcommand is specified) to
pass `signer_mode` to `ensure_daemons` and `run_tui`:

```rust
let signer_mode = cli.signer;
let daemons = ensure_daemons(
    &paypunkd_socket,
    &keypunkd_socket,
    &bridge_socket,
    signer_mode,
)
.await?;

run_tui(&paypunkd_socket, Some(shutdown), signer_mode).await?;
```

### 4. `cli/src/main.rs` — Subcommands that spawn daemons

Any subcommand that calls `ensure_daemons` (e.g., `SubmitTransfer`,
`GetBalance`, etc.) should also pass `signer_mode`:

```rust
let signer_mode = cli.signer;
let daemons = ensure_daemons(
    &paypunkd_socket,
    &keypunkd_socket,
    &bridge_socket,
    signer_mode,
)
.await?;
```

### 5. `cli/src/main.rs` — Add bridge socket path

Define a default bridge socket path (e.g., `/tmp/paypunk-bridge.sock` or use
the data directory). Add it alongside the existing `paypunkd_socket` and
`keypunkd_socket` constants.

### 6. `config/` — Optional: Add `offline_signer` to `PaypunkConfig`

If the config crate already has a `PaypunkConfig` struct, add:

```rust
pub struct PaypunkConfig {
    // ... existing fields ...
    pub offline_signer: bool,
}
```

With a default of `false`. The CLI flag can override the config value.

## Acceptance criteria

1. `cargo build --workspace` succeeds.
2. `cargo test --workspace` passes.
3. `cargo fmt --all` produces no changes.
4. `paypunk --signer` spawns the bridge instead of keypunkd.
5. `paypunk` (without `--signer`) spawns keypunkd as before (unchanged behavior).
6. `run_tui` receives `signer_mode: true` when `--signer` is set.
7. paypunkd connects to the bridge socket when `--signer` is set.
8. The `DaemonGuard` correctly kills the bridge on exit in signer mode.

## Context

This is the final wiring step for the existing codebase. The `--signer` flag is
the user-facing switch between the two modes:
- **Without `--signer`**: keypunkd mode — preview in TUI, password prompt, sign.
- **With `--signer`**: signer mode — bridge relays to QR, signer app signs, TUI
  shows "Awaiting signer..." spinner.

The bridge binary (`paypunk bridge`) already exists and is a format-agnostic relay
that receives IPC frames, displays as QR, scans response QR, and POSTs to
`/response`. No changes to the bridge code are needed.

The `paypunkd` daemon already accepts a `--keypunkd-socket` argument to configure
which socket to connect to for signing. In signer mode, this points to the bridge
socket instead of the keypunkd socket.

## Verification

```bash
cargo build --workspace
cargo test --workspace
cargo fmt --all

# Manual smoke tests:
# Without --signer (should behave as before):
cargo run -- --help  # should show --signer flag

# With --signer (requires bridge):
# cargo run -- --signer  # should spawn bridge, TUI shows signer mode
```

After verification, move this file to `./project/done/08_step.md` and commit with:

```
git add -A && git commit -m "cli: add --signer flag to spawn bridge instead of keypunkd"
```