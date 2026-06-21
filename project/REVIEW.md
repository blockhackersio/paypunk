# Process spawning

- I want to make it so that users must use the cli and run `paypunk paypunkd` also `paypunk keypunkd` to start the daemons. Remove the other binary compilations.
- Remove the binary builds for paypunkd and keypunkd and make the only way to run those daemons through the cli.
- make it so that running `paypunk` first launches `paypunk keypunkd`  then it launches `paypunk paypunkd` then it launches `paypunk tui` you can use env::current_exe() to get thee location of the cli
