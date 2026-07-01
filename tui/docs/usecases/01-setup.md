# SetupScreen — Wallet Creation / Import

**File:** `tui/src/screens/setup.rs:32`

Two paths: **Create New Wallet** and **Import Existing Wallet**. Both end with `Nav::Replace(HomeScreen)` on success.

## Create New Wallet Flow

```mermaid
sequenceDiagram
    participant U as User
    participant TUI as SetupScreen
    participant API as RealWalletApi
    participant Client as paypunk-api Client
    participant paypunkd as paypunkd (IPC)
    participant keypunkd as keypunkd (IPC)

    Note over TUI: init() called
    TUI->>API: get_setup()
    API->>Client: generate_mnemonic()
    Client-->>API: Zeroizing<String> (12-word phrase)
    API-->>TUI: SetupData { new_mnemonic: [12 words], ... }

    Note over TUI: User sees mnemonic (ShowMnemonic step)

    U->>TUI: Enter (confirm saved)
    Note over TUI: VerifyMnemonic step — user types words #4, #8, #12

    U->>TUI: Enter (verification submitted)
    TUI->>TUI: Validate words against stored mnemonic
    Note over TUI: SetPassword step — user enters + confirms password

    U->>TUI: Enter (password submitted)
    Note over TUI: Creating step — spinner shown

    TUI->>API: submit_setup_create(SetupCreateInput { password })
    API->>Client: restore_seed(mnemonic, password)
    Client->>paypunkd: IpcMessage(RestoreSeed { encrypted_mnemonic, encrypted_password, client_pk })
    paypunkd->>keypunkd: forward RestoreSeed
    keypunkd->>keypunkd: decrypt password, validate mnemonic, derive seed, encrypt+persist seed
    keypunkd-->>paypunkd: SeedRestored
    paypunkd-->>Client: SeedRestored
    Client-->>API: Ok(())
    API->>Client: unlock(password)
    Client->>paypunkd: IpcMessage(Unlock { ... complex payload ... })
    paypunkd->>keypunkd: forward encrypted password + paths
    keypunkd->>keypunkd: decrypt seed, derive viewing keys for 30 paths per protocol
    keypunkd-->>paypunkd: viewing keys
    paypunkd->>paypunkd: decrypt DB, save pre-derived keys
    paypunkd-->>Client: UnlockSuccess { accounts_count }
    Client-->>API: Ok(accounts_count)
    API-->>TUI: Ok(())

    TUI->>TUI: Nav::Replace(HomeScreen)
```

## Import Existing Wallet Flow

```mermaid
sequenceDiagram
    participant U as User
    participant TUI as SetupScreen
    participant API as RealWalletApi
    participant Client as paypunk-api Client
    participant paypunkd as paypunkd (IPC)
    participant keypunkd as keypunkd (IPC)

    Note over TUI: init() called
    TUI->>API: get_setup()
    API-->>TUI: SetupData { import_methods: ["mnemonic"], ... }

    U->>TUI: Select "Import Existing Wallet"
    Note over TUI: ImportMnemonic step — 12-field grid

    U->>TUI: Enter (phrase entered)
    Note over TUI: ImportPassword step — set + confirm password

    U->>TUI: Enter (password submitted)

    TUI->>API: submit_setup_import(SetupImportInput { method: "mnemonic", secret: phrase, password })
    API->>Client: restore_seed(mnemonic, password)
    Client->>paypunkd: IpcMessage(RestoreSeed { encrypted_mnemonic, encrypted_password, client_pk })
    paypunkd->>keypunkd: forward RestoreSeed
    keypunkd->>keypunkd: decrypt, validate, derive seed, encrypt+persist
    keypunkd-->>paypunkd: SeedRestored
    paypunkd-->>Client: SeedRestored
    Client-->>API: Ok(())
    API->>Client: unlock(password)
    Client->>paypunkd: IpcMessage(Unlock { ... })
    paypunkd->>keypunkd: forward
    keypunkd-->>paypunkd: viewing keys
    paypunkd->>paypunkd: save pre-derived keys
    paypunkd-->>Client: UnlockSuccess
    Client-->>API: Ok(())
    API-->>TUI: Ok(())

    TUI->>TUI: Nav::Replace(HomeScreen)
```
