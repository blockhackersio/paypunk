# ReceiveScreen — Display Receiving Address

**File:** `tui/src/screens/receive.rs:15`

Shows the account's receiving address, format info, QR payload, and a simulated QR code.

```mermaid
sequenceDiagram
    participant U as User
    participant TUI as ReceiveScreen
    participant API as RealWalletApi
    participant Client as paypunk-api Client
    participant paypunkd as paypunkd (IPC)

    Note over TUI: init(account) called
    TUI->>API: receive_state(account_id)
    API->>Client: get_account(account_id)
    Client->>paypunkd: IpcMessage(GetAccount { id })
    paypunkd-->>Client: account
    API-->>TUI: ApiState::Loaded(ReceiveData { address, chain_id, address_format, qr_payload })

    Note over TUI: Renders: Address, Format, QR Payload, simulated QR box

    U->>TUI: 'c' (copy address)
    TUI->>TUI: arboard::Clipboard::set_text(address)
    TUI->>TUI: Show "Copied!" feedback

    U->>TUI: Esc
    TUI->>TUI: Nav::Pop
```

## Reactivation Flow

```mermaid
sequenceDiagram
    participant TUI as ReceiveScreen
    participant API as RealWalletApi

    Note over TUI: on_reactivate() called
    TUI->>API: refresh_receive(account_id)
    API-->>TUI: (clears cache)
    TUI->>API: receive_state(account_id)
    API-->>TUI: Fresh ReceiveData
```
