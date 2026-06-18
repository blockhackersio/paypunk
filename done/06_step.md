# Step 6: Refactor TUI event loop to async

## Description

Rewrite the TUI event loop to run on tokio with async event handling. Spawn a blocking task for crossterm event polling that sends events through a `tokio::sync::mpsc` channel. The main loop uses `tokio::select!` to process events.

## Files to modify

- `tui/src/lib.rs` — Refactor `run_tui` to be async, accept socket path, use tokio event loop
- `tui/src/main.rs` — Parse `--socket-path` CLI arg, make `main` async, call `run_tui().await`

## Acceptance Criteria

- [ ] `cargo build` succeeds with no warnings
- [ ] `cargo test` passes all tests
- [ ] `run_tui()` accepts `Option<String>` for socket path
- [ ] The TUI launches and renders screens correctly with the mock backend (no socket path)
- [ ] Event handling works (keyboard input, paste, resize)

## Detailed Steps

1. Open `tui/src/lib.rs`. Refactor as follows:

   - Add `use tokio::sync::mpsc;`
   - Change `run_tui()` signature to accept socket path and return a Result:
     ```rust
     pub async fn run_tui(socket_path: Option<String>) -> io::Result<()> {
     ```
   - Create the API client: if socket_path is Some, create `RealWalletApi` (we'll build this in the next step — for now, always use MockWalletApi):
     ```rust
     let api: Box<dyn WalletApi> = if let Some(_path) = socket_path {
         // Step 7 will replace this with RealWalletApi
         Box::new(MockWalletApi::new())
     } else {
         Box::new(MockWalletApi::new())
     };
     ```
   - Replace the synchronous event loop with an async one:
     ```rust
     let (event_tx, mut event_rx) = mpsc::channel::<Event>(256);
     let event_tx_clone = event_tx.clone();

     // Spawn blocking task for crossterm event reading
     tokio::task::spawn_blocking(move || {
         loop {
             if event::poll(std::time::Duration::from_millis(50)).unwrap_or(false) {
                 let evt = event::read().unwrap_or(Event::Resize(0, 0));
                 if event_tx_clone.blocking_send(evt).is_err() {
                     break; // receiver dropped
                 }
             } else {
                 // Send a tick event periodically to allow rendering updates
                 if event_tx_clone.blocking_send(Event::Resize(0, 0)).is_err() {
                     break;
                 }
             }
         }
     });

     while !app.should_quit {
         terminal.draw(|frame| render(frame, &mut app))?;

         if let Some(evt) = event_rx.recv().await {
             match evt {
                 Event::Key(key) if key.kind == KeyEventKind::Press => {
                     if key.code == KeyCode::Char('q') && app.screen_stack.len() <= 1 {
                         app.should_quit = true;
                     } else if key.modifiers.contains(KeyModifiers::CONTROL)
                         && key.code == KeyCode::Char('c')
                     {
                         app.should_quit = true;
                     } else {
                         app.handle_input(key).await?;
                         if app.screen_stack.is_empty() {
                             app.should_quit = true;
                         }
                     }
                 }
                 Event::Paste(text) => {
                     app.handle_paste(&text).await;
                 }
                 Event::Resize(_, _) => {}
                 _ => {}
             }
         }
     }
     ```
   - Remove the old synchronous `run_app` function (its logic is now inline in `run_tui`).
   - Add necessary imports: `use crossterm::event::KeyModifiers;`, `use crossterm::event::Event;`

2. Open `tui/src/main.rs`. Rewrite as:
   ```rust
   use clap::Parser;

   #[derive(Parser)]
   #[command(name = "paypunk-tui", about = "Paypunk Terminal UI")]
   struct Args {
       #[arg(short, long)]
       socket_path: Option<String>,
   }

   #[tokio::main]
   async fn main() -> std::io::Result<()> {
       let args = Args::parse();
       paypunk_tui::run_tui(args.socket_path).await
   }
   ```
   Add `clap` to `tui/Cargo.toml` dependencies:
   ```toml
   clap = { workspace = true, features = ["derive"] }
   ```

3. Run `cargo build` and fix any compilation errors.

4. Run `cargo test` and verify all tests pass.

5. Launch the TUI with `cargo run --bin paypunk-tui` and verify it renders correctly and responds to keyboard input.

## Completion

When this step is complete — **STOP. DO NOT START THE NEXT STEP.**

```bash
git add -A && git commit -m "step 6: refactor TUI event loop to async"

mv todo/06_step.md done/

cat >> todo/goal.md << 'EOF'

## Step 6 — Done

Refactored TUI event loop to async on tokio. Spawned blocking task for crossterm events, mpsc channel for async communication. Added `--socket-path` CLI arg to TUI binary.
EOF
```

## ⛔ STOP HERE. WAIT FOR INSTRUCTION. DO NOT READ OR EXECUTE STEP 7.
