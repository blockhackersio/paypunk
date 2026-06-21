- all branches in main cli.command are async why use block_on? Why not have a single async switch?
- cli.command None should NOT be the same as cli.command = Commands::Tui `paypunk tui` should assume the background daemons are running already

