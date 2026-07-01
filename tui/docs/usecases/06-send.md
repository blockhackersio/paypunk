# SendScreen — Create and Send a Transfer

**File:** `tui/src/screens/send.rs:78`

Four-step flow: Form → Review → Sending → Confirm. This is the most complex usecase, spanning TUI → API → paypunkd → keypunkd with two-phase authorization.

## Step 1: Form (enter recipient, amount, memo)

```mermaid
sequenceDiagram
    participant U as User
    participant TUI as SendScreen
    participant API as RealWalletApi
    participant Client as paypunk-api Client
    participant paypunkd as paypunkd (IPC)

    Note over TUI: init(account) called
    TUI->>API: send_state(account_id)
    API->>Client: get_account(account_id)
    Client->>paypunkd: IpcMessage(GetAccount { id })
    paypunkd-->>Client: account
    API->>Client: get_balance(caip10, asset)
    Client->>paypunkd: IpcMessage(GetBalance { address, asset })
    paypunkd-->>Client: Balance
    API-->>TUI: ApiState::Loaded(SendData { from_address, spendable_balance, decimals, chain_id })

    TUI->>API: get_address_book()
    API->>Client: list_accounts() + local address_book_entries
    Client->>paypunkd: IpcMessage(ListAccounts)
    paypunkd-->>Client: accounts
    API-->>TUI: AddressBookData { entries: [wallet accounts + saved contacts] }

    Note over TUI: Renders: To picker (with address book), Amount field, Memo field (Zcash only), Balance display

    U->>TUI: Fill in recipient (type or pick from dropdown), amount, memo
    U->>TUI: Enter (submit review)
```

## Step 2: Review (submit intent, show preview)

```mermaid
sequenceDiagram
    participant TUI as SendScreen
    participant API as RealWalletApi
    participant Client as paypunk-api Client
    participant paypunkd as paypunkd (IPC)
    participant keypunkd as keypunkd (IPC)
    participant Protocol as Protocol impl

    TUI->>API: submit_send_review(SendReviewInput { to_address, amount, token_id, chain_id, account_id, memo })
    API->>Client: get_account(account_id)
    Client->>paypunkd: IpcMessage(GetAccount { id })
    paypunkd-->>Client: account

    Note over API: Build Intent based on protocol
    API->>Client: submit_intent(intent, derivation_path)

    Client->>paypunkd: IpcMessage(SubmitIntent { intent, derivation_path })
    paypunkd->>Protocol: build(intent) → unsigned artifact
    paypunkd->>keypunkd: preview_artifact(raw_artifact, protocol_id, derivation_path)
    keypunkd->>Protocol: parse_artifact(raw_artifact) → ArtifactSummary
    keypunkd-->>paypunkd: ArtifactPreview { raw_artifact, parsed_summary, signature, keypunkd_public_key }
    paypunkd-->>Client: SignablePreview { raw_artifact, parsed_summary, keypunkd_signature, keypunkd_public_key }
    Client-->>API: (raw_artifact, parsed_summary, keypunkd_signature, keypunkd_public_key)

    Note over API: Deserialize ArtifactSummary from parsed_summary
    Note over API: Store PendingSend { raw_artifact, keypunkd_signature, keypunkd_public_key, derivation_path, protocol }

    API-->>TUI: SendReviewData { to_address, amount, fee_estimate, total_amount, chain_id, nonce }

    Note over TUI: Renders review screen: From, To, Amount, Fee, Nonce, Total, Chain
    Note over TUI: Shows password field for authorization

    U->>TUI: Enter password + Enter
    TUI->>TUI: Set step = Sending, store PendingSend { review, password }
```

## Step 3: Sending (approve + broadcast, via tick)

```mermaid
sequenceDiagram
    participant TUI as SendScreen
    participant API as RealWalletApi
    participant Client as paypunk-api Client
    participant paypunkd as paypunkd (IPC)
    participant keypunkd as keypunkd (IPC)
    participant Protocol as Protocol impl

    Note over TUI: tick() fires on next render cycle

    TUI->>API: submit_send_confirm(SendConfirmInput { reviewed, auth_confirmation: { type: "password", value }, signed_tx: "" })

    API->>API: Take pending PendingSend from Mutex
    API->>API: Add recipient to address book (local)

    API->>Client: approve_signature(raw_artifact, keypunkd_signature, password, derivation_path)

    Note over Client: Encrypt payload (raw_artifact + signature + password_hash) to keypunkd's public key

    Client->>paypunkd: IpcMessage(ApproveSignature { encrypted_payload, ephemeral_public_key, derivation_path })
    paypunkd->>keypunkd: authorize_artifact(encrypted_payload, ephemeral_public_key, derivation_path)
    keypunkd->>keypunkd: Decrypt payload, verify signature, decrypt seed, sign artifact
    keypunkd-->>paypunkd: SignatureApproved { signed_artifact }
    paypunkd-->>Client: signed_artifact
    Client-->>API: Ok(signed_artifact)

    API->>Client: broadcast_transaction(protocol, signed_artifact)
    Client->>paypunkd: IpcMessage(BroadcastTransaction { protocol, raw_tx })
    paypunkd->>Protocol: finalize(signed_artifact) → finalized bytes
    paypunkd->>Protocol: broadcast(finalized_bytes) → tx_hash
    paypunkd-->>Client: TransactionBroadcasted { tx_hash }
    Client-->>API: Ok(tx_hash)

    Note over API: Build block explorer URL from protocol metadata template

    API-->>TUI: SendResult { tx_hash, status: "broadcasted", block_explorer_url }
    TUI->>TUI: Set step = Confirm
```

## Step 4: Confirm (show result)

```mermaid
sequenceDiagram
    participant U as User
    participant TUI as SendScreen

    Note over TUI: Renders: ✓ Transaction Broadcasted, TX Hash, Status, Block Explorer URL

    U->>TUI: 'c' (copy TX hash)
    TUI->>TUI: arboard::Clipboard::set_text(tx_hash)
    TUI->>TUI: Show "Copied!" feedback

    U->>TUI: Enter / Esc
    TUI->>TUI: Nav::Pop (back to AssetsScreen)
```
