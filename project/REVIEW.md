# Fix the following 


## Logging is displayed

Running with `cargo run` shows logging at the bottom of the tui:
```
2026-06-21T13:48:24.091538Z  INFO keypunkd::run: keypunkd listening on /tmp/keypunkd.sock
                                                                                         Error: Io(Os { code: 111, kind: ConnectionRefused, message: "Connection refused" })
```

## Running TUI -> Create New Wallet shows incorrect seed phrase

Shows fake canned seed phrase instead of correct newly generated seed phrase






