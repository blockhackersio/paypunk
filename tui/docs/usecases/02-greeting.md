# GreetingScreen — Initial Unlock

**File:** `tui/src/screens/greeting.rs:16`

Shown when `check_wallet_exists()` returns `true` (existing wallet found). Prompts for password, then navigates to HomeScreen.

```mermaid
sequenceDiagram
    participant U as User
    participant lib as run_tui()
    participant API as RealWalletApi
    participant Client as paypunk-api Client
    participant paypunkd as paypunkd (IPC)
    participant keypunkd as keypunkd (IPC)

    lib->>API: check_wallet_exists()
    API->>Client: check_wallet_exists()
    Client->>paypunkd: IpcMessage(HasSeed)
    paypunkd->>keypunkd: forward HasSeed
    keypunkd-->>paypunkd: HasSeed { exists: true }
    paypunkd-->>Client: HasSeed { exists: true }
    Client-->>API: true
    API-->>lib: true

    lib->>GreetingScreen: init()
    Note over GreetingScreen: Empty — no-op init

    lib->>lib: Push GreetingScreen onto screen stack
    lib->>GreetingScreen: render() — shows password field

    U->>GreetingScreen: Type password + Enter

    GreetingScreen->>API: unlock(password)

    API->>Client: unlock(password)
    Note over Client: Creates ephemeral Keypair
    Client->>paypunkd: IpcMessage(GetKeypunkEncryptionKey)
    paypunkd-->>Client: KeypunkEncryptionKey { key }
    Client->>paypunkd: IpcMessage(GetPaypunkdEncryptionKey)
    paypunkd-->>Client: PaypunkdEncryptionKey { key }
    Note over Client: Encrypts password hash to both keys
    Client->>paypunkd: IpcMessage(GetSupportedProtocols)
    paypunkd-->>Client: SupportedProtocols { protocols, metadata }
    Note over Client: Builds 30 derivation paths per protocol
    Client->>paypunkd: IpcMessage(Unlock { encrypted_db_password, encrypted_keypunkd_password, paths, ... })
    paypunkd->>paypunkd: Decrypt DB with password
    paypunkd->>keypunkd: forward encrypted password + paths
    keypunkd->>keypunkd: Decrypt seed, derive viewing keys for each path
    keypunkd-->>paypunkd: viewing keys per (protocol, path)
    paypunkd->>paypunkd: Save pre-derived keys to DB
    paypunkd-->>Client: UnlockSuccess { accounts_count }
    Client-->>API: Ok(accounts_count)
    API-->>GreetingScreen: Ok(UnlockData { accounts_count })

    GreetingScreen->>GreetingScreen: Nav::Replace(Box::new(HomeScreen))
```
