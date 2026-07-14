# Architecture

> **Note:** This is a work in progress. The architecture is designed for extensibility, but only simple transfers (send/receive) are currently implemented. Some features described here (DB encryption, fee estimation) are planned but not yet built.

## Design Principles

1. **Signing/wallet separation** — Keys live in a separate process (`keypunkd`) or on an air-gapped device. The wallet daemon never holds key material.
2. **Multi-token by design** — Chain-specific logic is isolated behind `Protocol` and `SignerProtocol` traits. Adding a chain means implementing traits, not rearchitecting.
3. **Frontend agnostic** — The IPC layer means any frontend technology can connect. The TUI is a first draft; future frontends (Tauri, web, agent SDKs) use the same backend.
4. **Referential transparency** — Local and remote actors share the same `Recipient<IpcMessage>` type, so in-process tests and cross-process production use identical code paths.

## Process Model

```
┌─────────────────────────────────────────────────────────────────┐
│                        paypunk (CLI/TUI)                         │
│                                                                  │
│  ┌──────────┐  ┌──────────┐  ┌──────────────────────────────┐  │
│  │  TUI     │  │  CLI     │  │  API Client (paypunk-api)    │  │
│  │ (ratatui)│  │ (clap)   │  │  Client::connect(socket)     │  │
│  └────┬─────┘  └────┬─────┘  └──────────┬───────────────────┘  │
│       │             │                    │                      │
│       └─────────────┴────────────────────┘                      │
│                     │ IpcSender                                │
└─────────────────────┼─────────────────────────────────────────┘
                      │ Unix socket
                      │ (X25519 handshake + Blake2b MAC)
                      ▼
┌─────────────────────────────────────────────────────────────────┐
│                      paypunkd (app daemon)                       │
│                                                                  │
│  ┌─────────────┐  ┌──────────────┐  ┌──────────────────────┐   │
│  │ Paypunkd    │  │ Protocol     │  │ SQLite DB            │   │
│  │ actor       │──│ Service      │  │ (paypunkd.db)        │   │
│  │ (tactix)    │  │ (HashMap)    │  │                      │   │
│  └──────┬──────┘  └──┬───┬───┬──┘  └──────────────────────┘   │
│         │            │   │   │                                 │
│         │     ┌──────┘   │   └──────┐                          │
│         │     ▼          ▼          ▼                          │
│  ┌──────┴─────────────────────────────────────────────────┐   │
│  │ ZcashProtocol    EthereumProtocol    (future chains)   │   │
│  │ (Protocol trait)  (Protocol trait)                      │   │
│  └────────────────────────────────────────────────────────┘   │
│         │ IpcSender (to keypunkd)                               │
└─────────┼─────────────────────────────────────────────────────┘
          │ Unix socket
          │ (X25519 handshake + Blake2b MAC)
          ▼
┌─────────────────────────────────────────────────────────────────┐
│                     keypunkd (key daemon)                        │
│                                                                  │
│  ┌─────────────┐  ┌──────────────┐  ┌──────────────────────┐   │
│  │ Keypunkd    │  │ Signer       │  │ Seed Store           │   │
│  │ actor       │──│ Protocol     │  │ (seed.enc)           │   │
│  │ (tactix)    │  │ Service      │  │ Argon2id + AES-GCM   │   │
│  └─────────────┘  └──┬───┬──────┘  └──────────────────────┘   │
│                      │   │                                    │
│              ┌───────┘   └───────┐                            │
│              ▼                   ▼                            │
│  ┌──────────────────┐  ┌──────────────────┐                  │
│  │ ZcashSigner      │  │ EthereumSigner   │                  │
│  │ Protocol         │  │ Protocol         │                  │
│  │ (SignerProtocol) │  │ (SignerProtocol) │                  │
│  └──────────────────┘  └──────────────────┘                  │
└─────────────────────────────────────────────────────────────────┘
```

## Crate Dependency Graph

```
                    paypunk-types
                   /              \
          paypunk-ipc          paypunk-config
              |                    |
         paypunk-api               |
        /         \               |
  paypunkd    paypunk-tui         |
    /  \         |               |
  /    \    paypunk (CLI) ───────/
protocols  keypunkd
 / \         |
zcash eth   |
            |
       paypunk-bridge
```

## Actor Message Flow

### Two-phase signing (submit + approve)

```
User                CLI/TUI              paypunkd              keypunkd
 │                     │                     │                     │
 │  submit_intent      │                     │                     │
 ├────────────────────>│  SubmitIntent       │                     │
 │                     ├────────────────────>│                     │
 │                     │                     │  build(intent)      │
 │                     │                     │  → raw_artifact     │
 │                     │                     │  preview_artifact   │
 │                     │                     ├────────────────────>│
 │                     │                     │                     │ parse_artifact
 │                     │                     │                     │ → summary
 │                     │                     │  ArtifactPreview   │
 │                     │                     │<────────────────────┤
 │                     │  SignablePreview    │                     │
 │                     │<────────────────────┤                     │
 │  show preview       │                     │                     │
 │  enter password     │                     │                     │
 ├────────────────────>│  approve_signature  │                     │
 │                     ├────────────────────>│                     │
 │                     │                     │  authorize_artifact │
 │                     │                     ├────────────────────>│
 │                     │                     │                     │ decrypt seed
 │                     │                     │                     │ sign artifact
 │                     │                     │  SignatureApproved  │
 │                     │                     │<────────────────────┤
 │                     │                     │  finalize + broadcast│
 │                     │  tx_hash            │                     │
 │                     │<────────────────────┤                     │
 │  show result        │                     │                     │
 │<────────────────────┤                     │                     │
```

### Unlock flow (first time)

```
User        CLI/TUI         paypunkd              keypunkd
 │             │               │                     │
 │  unlock     │               │                     │
 ├────────────>│  Unlock       │                     │
 │             ├──────────────>│                     │
 │             │               │  ensure DB exists   │
 │             │               │  run migrations     │
 │             │               │  SELECT accounts    │
 │             │               │  (none found)       │
 │             │               │  bulk_export_keys   │
 │             │               ├────────────────────>│
 │             │               │                     │ decrypt seed
 │             │               │                     │ derive 30 keys/protocol
 │             │               │  viewing keys       │
 │             │               │<────────────────────┤
 │             │               │  INSERT pre_derived │
 │             │               │  derive addresses   │
 │             │               │  INSERT accounts    │
 │             │               │  sync_account       │
 │             │  UnlockSuccess│                     │
 │             │<──────────────┤                     │
 │  show home  │               │                     │
 │<────────────┤               │                     │
```

## IPC Layer

The `paypunk-ipc` crate provides:

- **`IpcSender`** — tactix actor that connects to a Unix socket, performs the X25519 handshake, and sends authenticated messages
- **`IpcReceiver`** — accepts connections, performs the server-side handshake, verifies MACs, dispatches to a handler actor
- **`UnixSocketTransport`** — 4-byte LE length-prefixed framing over `UnixStream`
- **`IpcMessage`** — tactix message carrying opaque `Vec<u8>` payload + sender public key

The crate carries raw bytes — serialization (postcard) is done by callers. This keeps the IPC layer chain-agnostic.

### Wire protocol

| Byte | Message type | Payload |
|------|-------------|---------|
| `0x00` | `MSG_GET_PUBLIC_KEY` | none |
| `0x01` | `MSG_PUBLIC_KEY` | 32-byte X25519 public key |
| `0x02` | `MSG_REGISTER_CLIENT` | 32-byte client public key |
| `0x03` | `MSG_REGISTER_CLIENT_ACK` | none |
| `0x04` | `MSG_APPLICATION` | `[postcard bytes][32-byte Blake2b MAC]` |

See [ADR-001](../../adr/001-ipc-auth-model.md) for the authentication design.

## Trait System

### `Protocol` (wallet side, in `paypunkd`)

15 required methods + 9 optional with defaults. Required: `build`, `finalize`, `broadcast`, `get_balance`, `validate_address`, metadata getters, `derive_address_from_viewing_key`. Optional: sync, history, fee estimation.

### `SignerProtocol` (signer side, in `keypunkd`)

3 required methods, no defaults: `export_viewing`, `parse_artifact`, `sign`.

### Registration

- `paypunkd` — `ProtocolService::register(Box<dyn Protocol>)` in `run.rs`
- `keypunkd` — `ProtocolService::register(ProtocolId, Box<dyn SignerProtocol>)` in `run.rs`

Both registries are `HashMap`-based and require no changes when adding a chain — just call `register`.

## Data Storage

### keypunkd

| File | Format | Encryption |
|------|--------|------------|
| `{data_dir}/seed.enc` | `[salt(16B)][nonce(12B)][AES-256-GCM ciphertext]` | Argon2id key derivation + AES-256-GCM |
| `{data_dir}/seed.mnemonic.enc` | Same format | Same encryption |

Atomic writes via `.tmp` + rename.

### paypunkd

| File | Format | Encryption |
|------|--------|------------|
| `{data_dir}/paypunkd.db` | SQLite (rusqlite, bundled) | Plaintext (encryption planned) |
| `{data_dir}/.wallet_initialized` | Marker file | None |

Database tables: `accounts`, `pre_derived_keys`, `address_book`, `settings`, `signer_state`, `_migrations`.

## Background Sync

`paypunkd` spawns a background sync loop on startup that sends `Sync` messages to the Zcash `ScanActor` every 10 seconds. The `ScanActor` fetches compact blocks from lightwalletd via gRPC and feeds them to the `WalletDbActor` in 20-block chunks.

Ethereum has no background sync — it queries the RPC node on demand.

## Testing

The `tests/` workspace crate wires up the full actor chain (keypunkd + paypunkd) in-memory using `IpcSender::with_recipient`, which allows testing the complete IPC message flow without Unix sockets. See `tests/tests/integration_test.rs` and `tests/tests/pczt_test.rs`.

## See also

- [CONTEXT.md](CONTEXT.md) — domain glossary and terminology
- [ADD_PROTOCOL.md](ADD_PROTOCOL.md) — guide for adding new chains
- [SECURITY.md](SECURITY.md) — threat model and security boundaries
- [ADR-001](../../adr/001-ipc-auth-model.md) — IPC authentication design
