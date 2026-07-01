# LockScreen — Re-authentication

**File:** `tui/src/screens/lock.rs:17`

Shown after auto-lock timeout. User authenticates with password to return to HomeScreen.

**Persistence:** None. `get_lock()` returns hardcoded data (no DB read). `submit_lock()` is a no-op (no DB write). The screen exists only for the TUI-side lock UX — the daemon does not track lock state.

```mermaid
sequenceDiagram
    participant U as User
    participant TUI as LockScreen
    participant API as RealWalletApi

    Note over TUI: init() called
    TUI->>API: get_lock()
    Note over API: Returns hardcoded LockData — no IPC, no DB read
    API-->>TUI: LockData { auth_methods: { password_set: true }, failed_attempts: 0 }

    Note over TUI: Renders password field + failed attempts counter

    U->>TUI: Type password + Enter

    TUI->>API: submit_lock(LockInput { credential: { type: "password", value } })

    Note over API: RealWalletApi.submit_lock()
    Note over API: Always returns Ok(()) — no IPC, no DB write

    API-->>TUI: Ok(())

    TUI->>TUI: Nav::Replace(Box::new(HomeScreen))
```

On error (password wrong), the screen stays visible and displays the error message. On `Esc`, it returns `Nav::Pop`.
