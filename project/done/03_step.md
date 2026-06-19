# Step 03: Add daemon subcommands and fix shutdown propagation

**Prerequisites**: Step 02 (config wired into binaries)

## Goal

Add `paypunk keypunkd` and `paypunk paypunkd` subcommands so daemons can be launched independently. Fix the shutdown AtomicBool so it's wired to the TUI event loop, ensuring `ctrl+c` properly kills child daemons.

## Key files

- `cli/src/main.rs:23-84` — `Commands` enum and existing subcommands
- `cli/src/main.rs:86-124` — `spawn_daemons()` / `kill_daemons()`
- `cli/src/main.rs:126-153` — `main()` with TUI and ctrl+c handling
- `tui/src/lib.rs:24-104` — `run_tui()` event loop

## Tasks

1. Add `Commands::Keypunkd` variant to CLI (`cli/src/main.rs`):
   - Accepts `--socket-path`, `--data-dir` args (with config defaults via `ConfigLoader`)
   - Spawns `keypunkd` binary as child process (use `std::process::Command`)
   - Forwards `SIGINT`/`SIGTERM` to child (capture ctrl+c, kill child)
   - Waits for child to exit with `.wait()`

2. Add `Commands::Paypunkd` variant to CLI:
   - Accepts `--socket-path`, `--keypunkd-socket`, `--rpc-url`, `--data-dir` args (with config defaults)
   - Spawns `paypunkd` binary as child process
   - Forwards signals to child
   - Waits for child to exit

3. Fix shutdown in TUI mode:
   - Pass the `shutdown` AtomicBool to `paypunk_tui::run_tui()` so the TUI event loop can check it
   - When `shutdown` is true (ctrl+c captured), the TUI should break its event loop
   - `kill_daemons()` is still called after TUI exits

4. Update `paypunk_tui::run_tui()` signature to accept `Option<Arc<AtomicBool>>` for shutdown signal

5. Update `tui/src/lib.rs` event loop to check the shutdown flag on each iteration

## Cross-cutting concerns

- Daemon subcommands should NOT auto-spawn the other daemon — just run the single process
- Signal handling: use `tokio::signal::ctrl_c()` for async, `kill()` for child process
- Both daemons must be on `$PATH` or the binary path must be resolved
- `cli/src/main.rs:136-138` — the existing ctrl+c spawn sets `shutdown=true` but the TUI never checks it. Wire it.
- `tui/src/lib.rs:71-97` — the event loop's `while !app.should_quit` should also check the external shutdown flag

## Verification

```bash
cargo check -p paypunk
# Manual: start keypunkd in one terminal, paypunkd in another, then paypunk tui
```

## Acceptance Criteria

- [ ] `cargo check` succeeds
- [ ] `cargo test` passes
- [ ] `paypunk keypunkd` launches keypunkd and it binds its socket
- [ ] `paypunk paypunkd` launches paypunkd and it binds its socket
- [ ] `paypunk tui` still auto-spawns both daemons
- [ ] `ctrl+c` during `paypunk tui` kills both daemon child processes
- [ ] `ctrl+c` during `paypunk keypunkd` kills only keypunkd
- [ ] `ctrl+c` during `paypunk paypunkd` kills only paypunkd
- [ ] Code is committed with message: "feat: add keypunkd/paypunkd subcommands, fix shutdown propagation"
