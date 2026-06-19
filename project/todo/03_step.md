# Step 03: Add daemon subcommands and fix shutdown propagation

## Goal

Add `paypunk keypunkd` and `paypunk paypunkd` subcommands so daemons can be launched independently. Fix the shutdown AtomicBool so it's wired to the TUI event loop, ensuring `ctrl+c` properly kills child daemons.

## Tasks

1. Add `Commands::Keypunkd` variant to CLI:
   - Accepts `--socket-path`, `--data-dir` args (with config defaults)
   - Spawns `keypunkd` binary as child process
   - Forwards `SIGINT`/`SIGTERM` to child
   - Waits for child to exit

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

5. Update `tui/src/lib.rs` event loop to check the shutdown flag

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
