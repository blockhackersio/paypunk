# Paypunk Technical Specification

## Architecture Overview

Three-process architecture with an intent-based flow:

```
paypunk (CLI/TUI) → api → ipc → paypunkd → ipc → keypunkd
```

Each process has a single responsibility:
- **api**: Public library. Constructs `Intent` values, communicates with paypunkd via IPC. Hides all actor/IPC details from consumers.
- **paypunkd**: App daemon. Hosts `Protocol` implementations. Receives intents, builds unsigned artifacts, forwards to keypunkd for signing, finalizes signed artifacts.
- **keypunkd**: Key daemon. Hosts `SignerProtocol` implementations. Holds decrypted seed in protected memory. Parses artifacts for user preview, signs upon password-authenticated approval.

---

## Intent Layer

The system is driven by a strongly-typed, nested `Intent` enum. Protocol variants are known at compile time — no dynamic dispatch across protocols at the Intent level.

```rust
/// Top-level intent enum. One variant per supported protocol.
enum Intent {
    Zcash(ZcashIntent),
    Ethereum(EthereumIntent),
}

enum ZcashIntent {
    Transfer {
        to: String,                // raw Zcash address: "zs1..."
        amount: String,            // human-readable: "1.5"
        account: u32,              // wallet account number
        memo: Option<String>,      // 512-byte note memo
    },
}

enum EthereumIntent {
    Transfer {
        to: String,                // raw address: "0x..."
        amount: String,            // human-readable: "0.05"
        account: u32,              // wallet account number
        data: Option<String>,      // hex-encoded calldata
    },
    ContractCall {
        to: String,
        amount: String,
        account: u32,
        data: String,
    },
}
```

All address and asset fields within intents are raw protocol-level strings. CAIP standards are used for cross-boundary identification (see CAIP Parsers below).

---

## CAIP Parsers

The `types` crate provides parsers for CAIP standards, used internally by protocol impls and API-layer validation:

- **CAIP-2**: Blockchain ID (`namespace:reference` → e.g. `eip155:1`, `zcash:mainnet`)
- **CAIP-10**: Account ID (`chain_id:account_address` → e.g. `eip155:1:0x...`)
- **CAIP-19**: Asset ID (`chain_id:asset_namespace:asset_reference` → e.g. `eip155:1/erc20:0x...`)

Parsers live in `types::caip` and provide:
- Validation (is this string well-formed?)
- Extraction (give me the chain_id, namespace, reference)
- Construction (build a CAIP string from components)

---

## Protocol Trait

```rust
trait Protocol: Send + Sync {
    fn protocol_id(&self) -> ProtocolId;
    fn build(&self, intent: &Intent) -> Result<Vec<u8>, String>;
    fn finalize(&self, signed: &[u8]) -> Result<Vec<u8>, String>;
    fn validate_address(&self, address: &str) -> bool;
    fn get_balance(&self, address: &str, asset: &str) -> Result<Balance, String>;
}
```

- `build(intent)` — receives the full `Intent` enum, matches its protocol variant, produces a canonical unsigned artifact (PCZT for Zcash, RLP-encoded tx for Ethereum). Returns raw bytes.
- `finalize(signed)` — takes the signed artifact, produces broadcast-ready bytes.
- `validate_address(addr)` — returns whether the string is a valid address for this chain.
- `get_balance(address, asset)` — takes CAIP-10 address and CAIP-19 asset, returns balance.

Each impl deserializes the intent into its strong concrete type internally. The trait is generic; implementations are specific.

---

## SignerProtocol Trait

```rust
trait SignerProtocol: Send + Sync {
    fn chain(&self) -> ChainId;
    fn export_viewing(&self, path: &[u8]) -> Result<Vec<u8>, String>;
    fn parse_artifact(&self, artifact: &[u8]) -> Result<Vec<u8>, String>;
    fn sign(&self, seed: &[u8; 64], artifact: &[u8]) -> Result<Vec<u8>, String>;
}
```

- `chain()` — returns the CAIP-2 chain identifier.
- `export_viewing(path)` — derives and returns chain-specific viewing key material (Orchard FVK for Zcash, SEC1 pubkey for Ethereum, xpub for Bitcoin). The `path` encodes the derivation path (e.g. account number).
- `parse_artifact(artifact)` — parses the unsigned artifact into a serialized `ArtifactSummary` for user preview. Deterministic — same artifact always produces the same summary.
- `sign(seed, artifact)` — signs the artifact with the decrypted seed. Returns signed artifact bytes.

---

## ArtifactSummary

The structured type returned by `parse_artifact`, serialized with postcard:

```rust
struct ArtifactSummary {
    to: String,
    amount: String,
    fee: String,
    memo: Option<String>,
    protocol: ProtocolId,
}
```

This is what the user sees and approves. The API verifies that `H(raw, parsed)` matches keypunkd's signature before displaying it.

---

## Two-Phase Authorization Flow

### Phase 1: Submit & Preview

```
┌─────────┐     ┌──────────┐     ┌──────────┐     ┌──────────┐
│ paypunk │     │    api   │     │ paypunkd │     │ keypunkd │
│ (CLI)   │     │ (lib)    │     │ (daemon) │     │ (daemon) │
└────┬────┘     └────┬─────┘     └────┬──────┘     └────┬─────┘
     │                │                │                 │
     │ 1. SubmitIntent│                │                 │
     │  (intent)      │                │                 │
     │───────────────>│                │                 │
     │                │ 2. SubmitIntent│                 │
     │                │  (intent)      │                 │
     │                │───────────────>│                 │
     │                │                │ 3. Protocol::build(intent)
     │                │                │    → raw artifact
     │                │                │                 │
     │                │                │ 4. PreviewArtifact│
     │                │                │  (raw, protocol) │
     │                │                │─────────────────>│
     │                │                │                 │
     │                │                │   5. SignerProtocol::parse_artifact(raw)
     │                │                │      → parsed summary
     │                │                │   6. sig = sign(kp_sk, H(raw, parsed))
     │                │                │                 │
     │                │                │ 7. ArtifactPreview│
     │                │                │  (raw, parsed, sig, kp_pk)
     │                │                │<─────────────────│
     │                │                │                 │
     │                │ 8. SignablePreview               │
     │                │  (raw, parsed, sig, kp_pk)       │
     │                │<────────────────                 │
     │ 9. return      │                │                 │
     │ (raw,parsed,   │                │                 │
     │  sig, kp_pk)   │                │                 │
     │<───────────────│                │                 │
     │                │                │                 │
     │ 10. Verify:    │                │                 │
     │  H(raw,parsed) │                │                 │
     │  matches sig   │                │                 │
     │  using kp_pk   │                │                 │
     │                │                │                 │
     │ 11. Show parsed│                │                 │
     │  summary to    │                │                 │
     │  user          │                │                 │
```

### Phase 2: Approve & Sign

```
┌─────────┐     ┌──────────┐     ┌──────────┐     ┌──────────┐
│ paypunk │     │    api   │     │ paypunkd │     │ keypunkd │
│ (CLI)   │     │ (lib)    │     │ (daemon) │     │ (daemon) │
└────┬────┘     └────┬─────┘     └────┬──────┘     └────┬─────┘
     │                │                │                 │
     │ 12. User       │                │                 │
     │  approves,     │                │                 │
     │  enters pw     │                │                 │
     │───────────────>│                │                 │
     │                │ 13. Encrypt:   │                 │
     │                │  E(kp_pk,      │                 │
     │                │    ephem_pk,   │                 │
     │                │    raw, sig,   │                 │
     │                │    pw)         │                 │
     │                │                │                 │
     │                │ 14. ApproveSignature             │
     │                │  (encrypted,   │                 │
     │                │   ephem_pk)    │                 │
     │                │───────────────>│                 │
     │                │                │ 15. AuthorizeArtifact│
     │                │                │  (encrypted, ephem_pk)│
     │                │                │─────────────────>│
     │                │                │                 │
     │                │                │   16. Decrypt with kp_sk
     │                │                │   17. Re-parse raw → parsed'
     │                │                │   18. Verify H(raw, parsed') == sig
     │                │                │   19. Decrypt seed with pw
     │                │                │   20. SignerProtocol::sign(seed, raw)
     │                │                │                 │
     │                │                │ 21. ArtifactAuthorized│
     │                │                │  (signed_artifact)│
     │                │                │<─────────────────│
     │                │                │                 │
     │                │                │ 22. Protocol::finalize(signed)
     │                │                │     → broadcast-ready bytes
     │                │                │                 │
     │                │ 23. SignatureApproved            │
     │                │  (signed_artifact)               │
     │                │<────────────────                 │
     │ 24. return     │                │                 │
     │ signed artifact│                │                 │
     │<───────────────│                │                 │
```

### Security Properties

- **End-to-end password protection**: The password is encrypted to keypunkd's public key using an ephemeral API keypair. paypunkd never sees the plaintext password.
- **WYSIWYS**: keypunkd parses the artifact and produces the human-readable summary. The signature over `H(raw, parsed)` is verified by both the API (before showing the user) and keypunkd (before signing). A compromised paypunkd cannot swap artifacts.
- **No persistent state in keypunkd**: The raw artifact is returned to the API in the preview phase and sent back in the approval phase. keypunkd does not need to remember anything between phases.
- **No replay protection (v1)**: Replaying an approval message re-signs the same artifact. This is acceptable because the artifact is already consumed on chain after broadcast.

---

## IPC Messages

### paypunkd messages (api ↔ paypunkd)

```rust
enum PaypunkdRequest {
    // Intent flow
    SubmitIntent { intent: Intent },
    ApproveSignature { encrypted_payload: Vec<u8>, ephemeral_public_key: [u8; 32] },
    GetBalance { address: String, asset: String },

    // Seed management (unchanged)
    GetKeypunkEncryptionKey,
    GenerateSeed { encrypted_password: Vec<u8>, client_public_key: [u8; 32] },
    RestoreSeed { encrypted_mnemonic: Vec<u8>, encrypted_password: Vec<u8>, client_public_key: [u8; 32] },
    Unlock { encrypted_password: Vec<u8>, client_public_key: [u8; 32] },
    Lock,
}

enum PaypunkdResponse {
    // Intent flow
    SignablePreview { raw_artifact: Vec<u8>, parsed_summary: Vec<u8>, keypunkd_signature: Vec<u8>, keypunkd_public_key: [u8; 32] },
    SignatureApproved { signed_artifact: Vec<u8> },
    Balance { balance: Balance },

    // Seed management (unchanged)
    KeypunkEncryptionKey { key: [u8; 32] },
    SeedGenerated { encrypted_mnemonic: Vec<u8> },
    SeedRestored,
    Unlocked,
    Locked,
    Error { message: String },
}
```

### keypunkd messages (paypunkd ↔ keypunkd)

```rust
enum KeypunkdRequest {
    // Intent flow
    PreviewArtifact { raw_artifact: Vec<u8>, protocol: ProtocolId },
    AuthorizeArtifact { encrypted_payload: Vec<u8>, ephemeral_public_key: [u8; 32] },
    ExportViewingKey { protocol: ProtocolId, account: u32 },

    // Seed management (unchanged)
    GetEncryptionKey,
    GenerateSeed { encrypted_password: Vec<u8>, client_public_key: [u8; 32] },
    RestoreSeed { encrypted_mnemonic: Vec<u8>, encrypted_password: Vec<u8>, client_public_key: [u8; 32] },
    Unlock { encrypted_password: Vec<u8>, client_public_key: [u8; 32] },
    Lock,
}

enum KeypunkdResponse {
    // Intent flow
    ArtifactPreview { raw_artifact: Vec<u8>, parsed_summary: Vec<u8>, signature: Vec<u8>, keypunkd_public_key: [u8; 32] },
    ArtifactAuthorized { signed_artifact: Vec<u8> },
    ViewingKey { key: Vec<u8> },

    // Seed management (unchanged)
    EncryptionKey { key: [u8; 32] },
    SeedGenerated { encrypted_mnemonic: Vec<u8> },
    SeedRestored,
    Unlocked,
    Locked,
    Error { message: String },
}
```

---

## Encryption Details

### Approval payload encryption

When the user approves (Phase 2, step 13):

1. API generates an ephemeral X25519 keypair `(ephem_sk, ephem_pk)`
2. API computes shared secret: `X25519(ephem_sk, kp_pk)`
3. API derives encryption key from shared secret via Blake2b KDF
4. API encrypts `(raw, sig, pw)` using the derived key (ChaCha20-Poly1305 or AES-256-GCM)
5. API sends `(encrypted_payload, ephem_pk)` to paypunkd → keypunkd

keypunkd decrypts:
1. Computes shared secret: `X25519(kp_sk, ephem_pk)`
2. Derives same encryption key
3. Decrypts to get `(raw, sig, pw)`

This is standard ECIES-style encryption. No persistent key exchange needed.

---

## Data Model (unchanged from current)

```rust
struct Address(pub String);
struct Amount(pub u64);
struct TransferId(pub String);
struct BlockHeight(pub u64);

struct Balance {
    spendable: Amount,
    pending: Amount,
    total: Amount,
}

enum TransactionStatus {
    Pending,
    Confirmed(BlockHeight),
    Failed(String),
}

struct Transfer {
    id: TransferId,
    from: String,
    to: String,
    amount: Amount,
    fee: Amount,
    memo: Option<String>,
    status: TransactionStatus,
    created_at: u64,
}
```

---

## Implementation Order

### Phase 1: Types
1. Define `Intent`, `ZcashIntent`, `EthereumIntent` enums in `types` crate
2. Define `ArtifactSummary` struct in `types` crate
3. Implement CAIP-2, CAIP-10, CAIP-19 parsers in `types::caip`

### Phase 2: Traits
4. Refactor `Protocol` trait to `{ build, finalize, validate_address, get_balance }`
5. Refactor `SignerProtocol` trait to `{ chain, export_viewing, parse_artifact, sign }`

### Phase 3: IPC
6. Update `paypunkd` and `keypunkd` message types
7. Update `PaypunkService` and `KeypunkService` with new methods

### Phase 4: Protocol implementations
8. Update `ZcashProtocol` and `EthereumProtocol` to new trait signatures
9. Implement `parse_artifact` for both protocols

### Phase 5: API
10. Update `api` crate with `submit_intent`, `approve_signature` functions
11. Implement encryption/decryption for approval payload

### Phase 6: CLI/TUI
12. Wire up the two-phase flow in the CLI
13. Display `ArtifactSummary` to user and collect password
