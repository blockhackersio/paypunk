- ~~all branches in main cli.command are async why use block_on? Why not have a single async switch?~~ Fixed: single `rt.block_on` wrapping an async match
- ~~cli.command None should NOT be the same as cli.command = Commands::Tui `paypunk tui` should assume the background daemons are running already~~ Fixed: `None` spawns daemons + TUI; `paypunk tui` assumes daemons are running

