# SettingsScreen — Settings Management

**File:** `tui/src/screens/settings.rs:21`

Two sub-actions: **Main** (edit preferences) and **RevealPhrase** (authenticate to show mnemonic).

## Main Settings Flow

```mermaid
sequenceDiagram
    participant U as User
    participant TUI as SettingsScreen
    participant API as RealWalletApi

    Note over TUI: init() called
    TUI->>API: get_settings()
    API-->>TUI: SettingsData { security: { auto_lock_minutes: 5 }, fiat_currency: "USD", app_version: "0.1.0" }

    Note over TUI: Renders: Auto-Lock field, Fiat Currency field, "Reveal Recovery Phrase" option, "Save Settings" option

    U->>TUI: Edit Auto-Lock value
    U->>TUI: Edit Fiat Currency value
    U->>TUI: Navigate to "Save Settings" + Enter

    TUI->>API: submit_settings(SettingsInput { updated_security: { auto_lock_minutes }, fiat_currency })

    Note over API: RealWalletApi.submit_settings() — no-op, always returns Ok(())

    API-->>TUI: Ok(())

    U->>TUI: Esc
    TUI->>TUI: Nav::Pop
```

## Reveal Recovery Phrase Flow

```mermaid
sequenceDiagram
    participant U as User
    participant TUI as SettingsScreen
    participant API as RealWalletApi

    Note over TUI: In Main action, focus on "Reveal Recovery Phrase" + Enter
    TUI->>TUI: Set action = RevealPhrase

    Note over TUI: Renders password field for authentication

    U->>TUI: Type password + Enter

    TUI->>API: submit_reveal_phrase(RevealPhraseInput { auth_type: "password", value })

    Note over API: RealWalletApi — always returns Err("reveal phrase not yet supported via real API")

    alt Mock API (development/testing)
        API-->>TUI: Ok(vec!["ribbon", "velvet", ...])  (12 words)
        Note over TUI: Renders 12-word grid with warning: "Never share your recovery phrase"
    else Real API (production)
        API-->>TUI: Err("reveal phrase not yet supported via real API")
        Note over TUI: Shows error message
    end

    U->>TUI: Esc
    TUI->>TUI: Set action = Main, clear phrase
```
