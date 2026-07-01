# LockScreen — Re-authentication

**File:** `tui/src/screens/lock.rs:17`

Shown after auto-lock timeout. User authenticates with password to return to HomeScreen.

```mermaid
sequenceDiagram
    participant U as User
    participant TUI as LockScreen
    participant API as RealWalletApi

    Note over TUI: init() called
    TUI->>API: get_lock()
    API-->>TUI: LockData { auth_methods: { password_set: true }, failed_attempts: 0 }

    Note over TUI: Renders password field + failed attempts counter

    U->>TUI: Type password + Enter

    TUI->>API: submit_lock(LockInput { credential: { type: "password", value } })

    Note over API: RealWalletApi.submit_lock()
    Note over API: Always returns Ok(()) — no IPC call

    API-->>TUI: Ok(())

    TUI->>TUI: Nav::Replace(Box::new(HomeScreen))
```

On error (password wrong), the screen stays visible and displays the error message. On `Esc`, it returns `Nav::Pop`.
