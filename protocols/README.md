# The `Protocol` + `SignerProtocol` Traits: A Two-Sided Multichain Interface

A design document for the Rust traits that unify transaction construction and
signing across fundamentally different blockchain paradigms — UTXO, account-model,
and shielded — split across two processes for security compartmentalization.

---

## 1. Motivation

A multichain wallet must sign transactions for blockchains that differ on every
axis: key derivation, address encoding, transaction format, and signing algorithm.
Bitcoin uses UTXO-based transactions signed with secp256k1. Ethereum uses
account-nonce transactions signed with secp256k1 over Keccak-256 hashes. Zcash
Orchard uses shielded note-based transactions authorised with RedPallas signatures
and proven with Halo 2 zero-knowledge proofs.

Attempting to unify these behind a single `sign(input, key) -> output` function
fails at Zcash's shielded protocols, where proof generation and signing are
separate steps that may run on separate devices. The PCZT format
(`pczt` crate in `librustzcash`) and Bitcoin's PSBT (BIP-174/370) both solve this
the same way: they define a **partially-built transaction** that passes through a
pipeline of **roles** — Creator, Constructor, Prover, Signer, Extractor — where
each role adds its contribution and hands the bundle onward.

Paypunk adds a second dimension: **process isolation**. The seed never touches
paypunkd. The signing key material lives only in keypunkd. This means the role
pipeline must be split across an IPC boundary.

The two traits mirror that split:

- **`Protocol`** (in paypunkd): address derivation, transaction creation, proving,
  finalization — all operations that need no private key material (or only the
  full viewing key, which is not sufficient to spend).
- **`SignerProtocol`** (in keypunkd): key derivation and signing — operations
  that require the seed or spending key.

---

## 2. The Traits

### 2.1. `Protocol` — Non-Signing Operations (paypunkd)

```rust
pub trait Protocol: Send + Sync {
    fn protocol_id(&self) -> ProtocolId;

    /// Derive an address from a public key and diversifier index.
    ///
    /// Bitcoin:    P2PKH/P2WPKH address from compressed public key
    /// Ethereum:   EIP-55 checksummed address from uncompressed key
    /// Zcash:      Unified Address from Orchard FVK bytes
    fn derive_address(&self, public_key: &[u8], index: u32) -> Result<String, String>;

    /// Finalize a signed transaction: compute binding signatures,
    /// strip metadata, emit broadcast-ready raw bytes.
    ///
    /// Bitcoin:    assemble witness, drop PSBT fields
    /// Ethereum:   RLP-encode the signed transaction
    /// Zcash:      compute binding sig, verify proofs, emit v5 tx
    fn finalize_transaction(&self, transaction: &[u8]) -> Result<Vec<u8>, String>;

    /// Build a new transaction with proofs already generated
    /// (Creator + Constructor + Prover roles).
    ///
    /// Bitcoin:    UTXO selection, change output, fee estimation → PSBT
    /// Ethereum:   gas estimation, nonce → unsigned EIP-1559 tx
    /// Zcash:      note selection, ZIP-317 fee → PCZT with proofs
    fn create_transaction(
        &self,
        public_key: &[u8],
        account: u32,
        to: &str,
        amount: u64,
        memo: Option<&str>,
    ) -> Result<Vec<u8>, String>;
}
```

**Key design choice: byte serialization.** All data crosses trait method
boundaries as `&[u8]` / `Vec<u8>`. The same serialized bytes flow over the
Unix socket. Each implementation internally parses into its native types
(e.g., `pczt::Pczt` for Zcash, RLP bytes for Ethereum). No associated types
— the traits stay simple and object-safe.

### 2.2. `SignerProtocol` — Signing Operations (keypunkd)

```rust
pub trait SignerProtocol: Send + Sync {
    fn protocol_id(&self) -> ProtocolId;

    /// Derive a public key from the seed for a given account.
    ///
    /// Returns raw public key bytes that the `Protocol` side can use
    /// for address derivation.
    ///
    /// Bitcoin:    BIP-32 m/84'/0'/account' → compressed secp256k1 pubkey
    /// Ethereum:   BIP-32 m/44'/60'/account'/0/0 → uncompressed secp256k1 pubkey
    /// Zcash:      ZIP-32 Orchard → FullViewingKey bytes
    fn derive_public_key(&self, seed: &[u8; 64], account: u32) -> Result<Vec<u8>, String>;

    /// Sign a partially-built transaction.
    ///
    /// The `transaction` bytes are a chain-specific partially-built format
    /// (e.g., PCZT for Zcash). The seed-derived key is used to produce
    /// authorization signatures for each spend.
    ///
    /// Bitcoin:           ECDSA/Schnorr sig per UTXO input → PSBT
    /// Ethereum:          secp256k1 sig over Keccak-256 tx hash
    /// Zcash Orchard:     RedPallas spend auth sig per Action → PCZT
    fn sign_transaction(
        &self,
        seed: &[u8; 64],
        account: u32,
        transaction: &[u8],
    ) -> Result<Vec<u8>, String>;
}
```

### 2.3. `ProtocolId` — Chain Dispatch

```rust
pub enum ProtocolId {
    Zcash,
    Bitcoin,
    Ethereum,
    Monero,
    Solana,
}
```

Each protocol crate registers itself with a `ProtocolId`. The `api` layer
dispatches calls to the correct implementation based on the requested asset.

---

## 3. Pipeline Orchestration

The WalletActor (in paypunkd) is the pipeline orchestrator. It holds the
`Protocol` impl and the IPC handle to keypunkd's `SignerProtocol`, and calls
them in the chain-specific order. The `api` layer exposes a single
`create_transfer` that hides this complexity — consumers never see the
individual pipeline steps.

The canonical flow for any chain is: **create → sign → finalize → broadcast**.
Proving is bundled into `create_transaction` — both steps need the same note
data and the FullViewingKey is already available as the `public_key` parameter,
so there's no reason to expose them separately.

## 4. Role Pipeline Across Processes

The role pipeline from PSBT/PCZT — `new -> prove -> authorize -> extract` —
is split across the two processes, with `prove` internal to `create`:

```
 paypunkd (Protocol)                 keypunkd (SignerProtocol)
 ─────────────────────                ─────────────────────────
 create_transaction()
 (Creator+Constructor+Prover)
                                     ──IPC──► sign_transaction()
                                     ◄──IPC── (Authorize)
 finalize_transaction()
 (Extractor)
```

| Role         | Trait Method           | Process   | Key Material Needed        |
|--------------|------------------------|-----------|----------------------------|
| Creator      | `create_transaction`   | paypunkd  | None (public key only)     |
| Constructor  | `create_transaction`   | paypunkd  | None (public key only)     |
| Prover       | `create_transaction`   | paypunkd  | FullViewingKey (not secret)|
| Authorize    | `sign_transaction`     | keypunkd  | Spending key (secret)      |
| Extractor    | `finalize_transaction` | paypunkd  | None                       |

### 4.1. The Full Transfer Flow

```
1. api.create_transfer(asset, to, amount, memo)
       │
2. paypunkd: derive_address via Protocol::derive_address (for change)
       │
3. paypunkd: Protocol::create_transaction → PCZT bytes (with proofs)
       │         (proving is internal to create_transaction)
4. paypunkd --IPC--> keypunkd: SignerProtocol::sign_transaction
       │                keypunkd: unlock seed, derive key, sign
       │         <--IPC-- return signed PCZT bytes
       │
5. paypunkd: Protocol::finalize_transaction → raw tx bytes
       │
6. broadcast via lightwalletd / RPC
```

---

## 4. Implementation: Zcash

Zcash is the primary target and the most complex case. It uses the PCZT format
(`pczt::Pczt`) as the wire format between all pipeline stages.

```rust
// protocols/zcash/src/protocol.rs

impl Protocol for ZcashProtocol {
    fn protocol_id(&self) -> ProtocolId { ProtocolId::Zcash }

    fn derive_address(&self, public_key: &[u8], index: u32) -> Result<String, String> {
        // Deserialize public_key as orchard::keys::FullViewingKey bytes.
        // Call FullViewingKey::address_at(index, Scope::External).
        // Encode as Unified Address with "u" HRP.
    }

    fn finalize_transaction(&self, transaction: &[u8]) -> Result<Vec<u8>, String> {
        // Parse bytes as pczt::Pczt.
        // Use pczt::roles::spend_finalizer::SpendFinalizer.
        // Verify proofs with orchard::circuit::VerifyingKey.
        // Compute binding signature.
        // Extract raw v5 transaction bytes.
    }

    fn create_transaction(
        &self, public_key: &[u8], account: u32,
        to: &str, amount: u64, memo: Option<&str>,
    ) -> Result<Vec<u8>, String> {
        // Use zcash_primitives::transaction::builder::Builder
        // with pczt::roles::io_finalizer to produce PCZT,
        // THEN prove inline via pczt::roles::prover before returning.
        //
        // Proving is bundled here because:
        //   - Both creation and proving need the same note/witness data
        //   - The FullViewingKey is already in public_key bytes
        //   - It eliminates a separate IPC round-trip
        //
        // Requires WalletDb for note selection and merkle paths.
    }
}
```

```rust
// protocols/zcash/src/protocol.rs

impl SignerProtocol for ZcashProtocol {
    fn protocol_id(&self) -> ProtocolId { ProtocolId::Zcash }

    fn derive_public_key(&self, seed: &[u8; 64], account: u32) -> Result<Vec<u8>, String> {
        // ZIP-32 Orchard derivation:
        //   SpendingKey::from_zip32_seed(seed, 133, account)
        //     .into() -> FullViewingKey
        // Return FVK bytes.
    }

    fn sign_transaction(
        &self, seed: &[u8; 64], account: u32,
        transaction: &[u8],
    ) -> Result<Vec<u8>, String> {
        // Parse bytes as pczt::Pczt.
        // Derive UnifiedSpendingKey from seed.
        // For each Orchard Action needing signature:
        //   Extract zip32_derivation from PCZT to identify account.
        //   Use Signer::sign_orchard(index, &ask).
        // Return serialized PCZT with spend_auth_sigs filled in.
    }
}
```

### 4.1. Key Hierarchy

Orchard's key tree, as implemented in the `orchard` crate:

```
orchard::keys::SpendingKey               (derived from seed via ZIP-32)
 ├── orchard::keys::SpendAuthorizingKey  (ask: Pallas scalar) — in keypunkd
 ├── SpendValidatingKey                  (ak: Pallas point)
 ├── NullifierDerivingKey                (nk: Pallas point)
 └── orchard::keys::FullViewingKey       (ak, nk, rivk) — crosses IPC
      └── orchard::keys::IncomingViewingKey (ivk)
           └── diversifier d → orchard::Address(d, pk_d)
```

Mapping to the two traits:

- **`SignerProtocol`** holds the seed and derives `SpendAuthorizingKey` from it
  at signing time. Never exposes the key — signs inside the process boundary.
- **`Protocol`** receives the `FullViewingKey` bytes (from
  `SignerProtocol::derive_public_key`) and uses it for address derivation and
  proof generation. The FVK is not sufficient to spend — it only allows
  viewing and proving.

### 4.2. Crate Mapping

| Trait concept           | `librustzcash` crate / type                    |
|-------------------------|-----------------------------------------------|
| Wire format             | `pczt::Pczt`                                  |
| Prover role (Protocol)  | `pczt::roles::prover` + `zcash_proofs::prover`|
| Signer role (Signer)    | `pczt::roles::signer`                         |
| Extractor role (Protocol)| `pczt::roles::tx_extractor`                  |
| Spend finalizer (Prot.) | `pczt::roles::spend_finalizer`                |
| Key derivation (Signer) | `orchard::keys::SpendingKey::from_zip32_seed` |
| Address derivation (Prot)| `orchard::keys::FullViewingKey::address_at`  |
| Proof system            | Halo 2 (`orchard::circuit`)                   |
| Signature scheme        | RedPallas (`orchard::keys::SpendAuthorizingKey`) |

---

## 5. Implementation: Ethereum

Ethereum is the simplest case. No proof system, no partially-built transaction
format — the "bundle" is just RLP-encoded transaction bytes.

```rust
// protocols/ethereum/src/protocol.rs

impl Protocol for EthereumProtocol {
    fn protocol_id(&self) -> ProtocolId { ProtocolId::Ethereum }

    fn derive_address(&self, public_key: &[u8], _index: u32) -> Result<String, String> {
        // Keccak-256 hash of the uncompressed public key (skip first byte).
        // Take last 20 bytes, EIP-55 checksum encode.
    }

    fn finalize_transaction(&self, transaction: &[u8]) -> Result<Vec<u8>, String> {
        Ok(transaction.to_vec()) // Already finalized after signing.
    }

    fn create_transaction(
        &self, _public_key: &[u8], _account: u32,
        _to: &str, _amount: u64, _memo: Option<&str>,
    ) -> Result<Vec<u8>, String> {
        Ok(vec![]) // Not yet implemented.
    }
}

impl SignerProtocol for EthereumProtocol {
    fn protocol_id(&self) -> ProtocolId { ProtocolId::Ethereum }

    fn derive_public_key(&self, seed: &[u8; 64], account: u32) -> Result<Vec<u8>, String> {
        // BIP-32 derivation on path m/44'/60'/{account}'/0/0
        // using bip32 crate with k256::ecdsa::SigningKey.
        // Return uncompressed secp256k1 public key bytes.
    }

    fn sign_transaction(
        &self, _seed: &[u8; 64], _account: u32,
        _transaction: &[u8],
    ) -> Result<Vec<u8>, String> {
        Err("Ethereum signing not yet implemented".to_string())
    }
}
```

**Key types.** `derive_public_key` runs BIP-32 on path `m/44'/60'/account'/0/0`
using the `bip32` crate with `k256::ecdsa::SigningKey`. Ethereum uses one
address per account — `index` is ignored in `derive_address`.

---

## 6. Why Two Traits, Not One

The monolithic single `Protocol` trait (with `derive_keys` + `sign`) assumes
one process holds all keys and performs all operations. This breaks in
Paypunk's three-process architecture:

**Security compartmentalization.** The seed lives in keypunkd, which runs as a
separate system user. If paypunkd is compromised, the attacker still cannot
spend funds — they only have access to view keys and can build/prove
transactions but cannot authorize them.

**Hardware wallets.** A Ledger can produce secp256k1 or RedPallas signatures
but cannot generate Halo 2 proofs — it lacks the memory and compute. The
`SignerProtocol` abstraction maps cleanly onto a hardware wallet: the device
implements `sign_transaction`, while the desktop runs `create_transaction`
(which includes proving) and `finalize_transaction`.

**Multisig / threshold signing.** Multiple signers each call
`sign_transaction` on the same bundle. Because the method takes and returns
serialized bytes, it composes naturally — pipe the output of one signer into
the input of the next.

**View-only wallets.** A watch-only wallet implements `Protocol` without
`SignerProtocol` — it can derive addresses, create transactions, and prove,
but never sign. This is the default state of paypunkd.

### 6.1. Comparison: Role Pipeline vs. Monolithic `sign()`

| Scenario                    | Monolithic `sign()` | Role pipeline (two traits) |
|-----------------------------|---------------------|---------------------------|
| Standard desktop wallet     | Works               | Works (extra round-trip)  |
| Hardware wallet             | Impossible          | Works (prove on desktop)  |
| Multisig                    | One-shot only       | N signers, N calls        |
| View-only wallet            | Can't exist         | Protocol only, no Signer  |
| Compromised paypunkd        | Funds lost          | Funds safe (key in kp)    |
| Compromised keypunkd        | —                   | Funds lost (but paypunkd isolated) |

---

## 7. The Hardware Wallet Flow

The two-trait split enables the following device-crossing flow for Orchard:

```
 paypunkd (Protocol)                Hardware Wallet (SignerProtocol)
 ─────────────────────               ─────────────────────────────────
 create_transaction()
   (includes proving)
       │
       ▼
    PCZT bytes (proofs present,      serialize & transmit
          no sigs)             ─────────────────────────►
                                       sign_transaction(pczt)
                                             │
                                             ▼
                                       PCZT bytes (proofs + sigs)
                                ◄─────────────────────────
       │
 finalize_transaction(pczt)
       │
       ▼
    Raw v5 transaction
       │
    broadcast
```

The critical insight: `create_transaction` (which includes proving) runs on a
machine with gigabytes of RAM and multi-second compute budget.
`sign_transaction` runs on a constrained device that can only do scalar
arithmetic on the Pallas curve. The PCZT serialization format bridges them.

---

## 8. Unified Zcash Transactions

Real Zcash transactions may contain transparent, Sapling, and Orchard
components simultaneously. The `pczt::Pczt` struct carries all three as
optional bundles. A unified Zcash implementation handles all three pools
inside a single `Protocol` + `SignerProtocol` pair:

```rust
impl Protocol for ZcashProtocol {
    // create_transaction builds a PCZT with transparent/Sapling/Orchard
    //   components based on available funds, then proves all actions inline.
    // finalize_transaction handles binding signatures for all pools.
}

impl SignerProtocol for ZcashProtocol {
    // derive_public_key returns the UnifiedSpendingKey-derived FVK.
    // sign_transaction signs transparent (secp256k1), Sapling (RedJubjub),
    //   and Orchard (RedPallas) spends from a single UnifiedSpendingKey.
}
```

This mirrors `zcash_keys::keys::UnifiedSpendingKey`, which bundles
transparent, Sapling, and Orchard spending keys derived from a single seed
via ZIP-32. `derive_public_key` calls `UnifiedSpendingKey::from_seed()`, and
each role method delegates to the per-pool logic internally.

The `derive_address` method produces a **Unified Address** (ZIP-316) — a
single address string (HRP `"u"`) that encodes receivers for all three pools.
A sender's wallet picks the most shielded receiver it supports.

---

## 9. Crate Dependency Map

For a multichain wallet integrating Zcash and Ethereum via the two traits:

```
paypunkd (Protocol trait)
 ├── paypunk-types          (trait definitions)
 ├── paypunk-chains-zcash   (Zcash Protocol impl)
 │    ├── pczt              (PCZT format, role implementations)
 │    │    └── orchard      (Orchard protocol types, Halo 2 circuit)
 │    ├── zcash_primitives  (transaction builder, consensus rules)
 │    ├── zcash_proofs      (Halo 2 prover for Orchard)
 │    ├── zcash_keys        (UnifiedAddress, FVK types)
 │    └── orchard           (circuit, keys, bundle types)
 │
 └── paypunk-chains-ethereum (Ethereum Protocol impl)
      └── sha3              (Keccak-256 for address derivation)

keypunkd (SignerProtocol trait)
 ├── paypunk-types          (trait definitions)
 ├── paypunk-chains-zcash   (Zcash SignerProtocol impl)
 │    ├── pczt              (PCZT parse/serialize, signer role)
 │    ├── zcash_keys        (UnifiedSpendingKey derivation)
 │    ├── orchard           (SpendAuthorizingKey, RedPallas)
 │    └── zip32             (ZIP-32 key derivation)
 │
 └── paypunk-chains-ethereum (Ethereum SignerProtocol impl)
      ├── bip32             (BIP-32 key derivation)
      └── k256              (secp256k1 for ECDSA)
```

---

## 10. What the Traits Do Not Cover

**Block scanning and note discovery.** The traits cover transaction
construction and signing. Discovering which notes belong to a wallet (trial
decryption of the blockchain) is handled by `zcash_client_backend`'s scanning
infrastructure and is orthogonal to signing.

**Fee policy.** The `create_transaction` method produces a transaction, but
the fee algorithm (ZIP-317 for Zcash, EIP-1559 for Ethereum) is internal to
each implementation. The trait does not prescribe a fee model.

**Network interaction.** Broadcasting, mempool management, and confirmation
tracking are transport-layer concerns outside the trait.

**View-only wallets.** A view-only wallet implements `Protocol` without
`SignerProtocol` — it can derive addresses and create/prove transactions but
cannot sign. This is enforced at the process level: paypunkd never holds the
seed.

**Transaction history and balance tracking.** The traits are
construction-oriented. Querying balances and history is handled by the
`WalletActor`'s database and is orthogonal.
