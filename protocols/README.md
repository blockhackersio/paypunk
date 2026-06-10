
# The `Protocol` Trait: A Role-Based Multichain Signing Interface

A design document for a Rust trait that unifies transaction construction and
signing across fundamentally different blockchain paradigms — UTXO, account-model,
and shielded — using the role-based pipeline established by PSBT and PCZT.

---

## 1. Motivation

A multichain wallet must sign transactions for blockchains that differ on every
axis: key derivation, address encoding, transaction format, and signing algorithm.
Bitcoin uses UTXO-based transactions signed with secp256k1. Ethereum uses
account-nonce transactions signed with secp256k1 over Keccak-256 hashes. Zcash
Orchard uses shielded note-based transactions authorised with RedPallas signatures
and proven with Halo 2 zero-knowledge proofs.

Attempting to unify these behind a single `sign(input, key) → output` function
fails at Zcash's shielded protocols, where proof generation and signing are
separate steps that may run on separate devices. The PCZT format
(`pczt` crate in `librustzcash`) and Bitcoin's PSBT (BIP-174/370) both solve this
the same way: they define a **partially-built transaction** that passes through a
pipeline of **roles** — Creator, Constructor, Prover, Signer, Extractor — where
each role adds its contribution and hands the bundle onward.

This document defines a `Protocol` trait that mirrors that role pipeline. Each
blockchain implements the trait once. The wallet layer calls the same methods
regardless of chain.

---

## 2. The Trait

```rust
pub trait Protocol {
    /// The partially-built transaction flowing between roles.
    /// Must be serializable — this crosses process/device boundaries.
    ///
    /// Bitcoin:     bitcoin::psbt::Psbt
    /// Ethereum:    a local UnsignedTransaction struct
    /// Zcash:       pczt::Pczt
    type Bundle: Clone;

    /// What the prover needs to generate proofs.
    ///
    /// Bitcoin/Ethereum:   ()
    /// Zcash Sapling:      sapling::keys::FullViewingKey
    /// Zcash Orchard:      orchard::keys::FullViewingKey
    type ProofKey;

    /// What the signer needs to authorise spends.
    ///
    /// Bitcoin:             secp256k1::SecretKey
    /// Ethereum:            secp256k1::SecretKey
    /// Zcash Transparent:   secp256k1::SecretKey
    /// Zcash Sapling:       sapling::keys::SpendAuthorizingKey
    /// Zcash Orchard:       orchard::keys::SpendAuthorizingKey
    type SigningKey;

    /// Chain-specific transaction inputs before construction.
    type SigningInput;

    /// Signed, broadcast-ready transaction bytes.
    type SigningOutput;

    /// Pre-sign analysis: input selection, fee estimation.
    type TransactionPlan;

    // ── Chain identity ──────────────────────────────────────

    fn chain_info(&self) -> ChainInfo;

    // ── Key derivation ──────────────────────────────────────

    /// Derive all key material for one account from a seed.
    ///
    /// Bitcoin:    BIP-32/44 → (SecretKey, PublicKey)
    /// Ethereum:   BIP-32/44 m/44'/60'/account' → (SecretKey, PublicKey)
    /// Zcash:      ZIP-32 → UnifiedSpendingKey →
    ///               (transparent SecretKey, Sapling ExtendedSpendingKey,
    ///                Orchard SpendingKey)
    fn derive_keys(
        &self,
        seed: &[u8],
        account: u32,
    ) -> Result<KeySet<Self>, Error>;

    // ── Address ─────────────────────────────────────────────

    /// Derive an address from the key set.
    ///
    /// The `diversifier_index` parameter:
    ///   Bitcoin:    address index in BIP-44 chain
    ///   Ethereum:   ignored (one address per account)
    ///   Zcash Sapling/Orchard: diversifier index (ZIP-316),
    ///                          generates unlinkable addresses
    ///                          from the same viewing key
    fn derive_address(
        &self,
        keys: &KeySet<Self>,
        diversifier_index: u32,
    ) -> Result<Address, Error>;

    fn validate_address(&self, address: &str) -> bool;

    // ── Planning ────────────────────────────────────────────

    /// Build a transaction plan: select inputs, estimate fees.
    ///
    /// Bitcoin:    UTXO selection, change output, fee estimation
    /// Ethereum:   gas estimation
    /// Zcash:      note selection across pools, fee per ZIP-317
    fn plan(
        &self,
        input: &Self::SigningInput,
    ) -> Result<Self::TransactionPlan, Error>;

    // ── Role pipeline ───────────────────────────────────────

    /// Creator + Constructor + IO Finalizer.
    /// Build the bundle structure and seal the set of inputs/outputs.
    fn new(
        &self,
        input: &Self::SigningInput,
        plan: &Self::TransactionPlan,
    ) -> Result<Self::Bundle, Error>;

    /// Prover role.
    /// Generate cryptographic proofs for each shielded action.
    ///
    /// Bitcoin/Ethereum: no-op — return bundle unchanged.
    /// Zcash Sapling:    Groth16 proofs via zcash_proofs
    /// Zcash Orchard:    Halo 2 proofs via orchard::circuit
    fn prove(
        &self,
        bundle: Self::Bundle,
        proof_key: &Self::ProofKey,
    ) -> Result<Self::Bundle, Error>;

    /// Signer role.
    /// Produce authorization signatures for each spend.
    ///
    /// Bitcoin:           ECDSA/Schnorr sig per UTXO input
    /// Ethereum:          secp256k1 sig over Keccak-256 tx hash
    /// Zcash Transparent: ECDSA sig per transparent input
    /// Zcash Sapling:     RedJubjub spend auth sig per Spend
    /// Zcash Orchard:     RedPallas spend auth sig per Action
    fn authorize(
        &self,
        bundle: Self::Bundle,
        signing_key: &Self::SigningKey,
    ) -> Result<Self::Bundle, Error>;

    /// Extractor role.
    /// Strip metadata, compute any final signatures, emit raw tx.
    ///
    /// Bitcoin:    assemble witness, drop PSBT fields
    /// Ethereum:   already signed after authorize, just serialize
    /// Zcash:      compute binding signature from accumulated
    ///             randomness, verify completeness, emit v5 tx
    fn extract(
        &self,
        bundle: Self::Bundle,
    ) -> Result<Self::SigningOutput, Error>;

    // ── Convenience ─────────────────────────────────────────

    /// Collapse the full pipeline for wallets holding all keys.
    fn sign(
        &self,
        input: &Self::SigningInput,
        keys: &KeySet<Self>,
    ) -> Result<Self::SigningOutput, Error> {
        let plan = self.plan(input)?;
        let bundle = self.new(input, &plan)?;
        let bundle = self.prove(bundle, &keys.proof_key)?;
        let bundle = self.authorize(bundle, &keys.signing_key)?;
        self.extract(bundle)
    }
}

pub struct KeySet<P: Protocol + ?Sized> {
    pub proof_key: P::ProofKey,
    pub signing_key: P::SigningKey,
    pub address_key: AddressKey,
}
```

---

## 3. Why Roles, Not `sign()`

The monolithic `sign(input, key) → output` model assumes one party holds all
keys and performs all operations. This breaks in three real scenarios:

**Hardware wallets.** A Ledger can produce secp256k1 signatures but cannot
generate Halo 2 proofs — it lacks the memory and compute. PCZT allows the
proof to be generated on a desktop, then the partially-proven bundle is sent
to the Ledger for signing, then returned for extraction.

**Multisig / threshold signing.** Multiple signers each call `authorize` on
the same bundle. Because `authorize` takes and returns the same `Bundle` type,
it composes naturally — call it N times for N signers.

**Delegation.** A watch-only wallet can `new` and `plan` without any
private key material, then serialize the `Bundle` and send it elsewhere for
proving and signing.

The role pipeline is `new → prove → authorize → extract`, but only
`prove` and `authorize` require key material. The other roles are
capability-free.

---

## 4. Implementation: Bitcoin

Bitcoin maps cleanly because PSBT *is* the partially-built transaction format
that the trait's `Bundle` type was modelled on.

```rust
impl Protocol for Bitcoin {
    type Bundle        = bitcoin::psbt::Psbt;
    type ProofKey      = ();                    // no proofs
    type SigningKey     = secp256k1::SecretKey;
    type SigningInput   = BitcoinSigningInput;
    type SigningOutput  = BitcoinSigningOutput;
    type TransactionPlan = BitcoinTransactionPlan;

    fn new(&self, input: &Self::SigningInput, plan: &Self::TransactionPlan)
        -> Result<Psbt, Error>
    {
        // Build unsigned transaction from selected UTXOs and outputs.
        // Populate PSBT fields: witness_utxo, bip32_derivation, sighash_type.
    }

    fn prove(&self, bundle: Psbt, _: &()) -> Result<Psbt, Error> {
        Ok(bundle) // No proofs in Bitcoin.
    }

    fn authorize(&self, mut bundle: Psbt, key: &secp256k1::SecretKey)
        -> Result<Psbt, Error>
    {
        // For each input:
        //   Compute sighash (BIP-143 for SegWit, BIP-341 for Taproot).
        //   Sign with ECDSA or Schnorr depending on script type.
        //   Insert signature into partial_sigs or tap_key_sig.
    }

    fn extract(&self, bundle: Psbt) -> Result<Self::SigningOutput, Error> {
        // For each input: assemble witness stack from partial_sigs + scripts.
        // Strip all PSBT metadata.
        // Serialize the final transaction.
    }
}
```

**Key types.** `derive_keys` runs BIP-32 derivation from the seed using the
chain's BIP-44 path (`m/84'/0'/account'` for native SegWit). `ProofKey` is
`()` — Bitcoin has no proof system. `SigningKey` is a standard secp256k1
secret key. The `diversifier_index` parameter in `derive_address` maps to the
address index in the BIP-44 external chain (`m/84'/0'/account'/0/index`).

---

## 5. Implementation: Ethereum

Ethereum is the simplest case. There is no partially-built format analogous
to PSBT — the "bundle" is just the unsigned transaction.

```rust
impl Protocol for Ethereum {
    type Bundle        = EthereumUnsignedTx;
    type ProofKey      = ();
    type SigningKey     = secp256k1::SecretKey;
    type SigningInput   = EthereumSigningInput;
    type SigningOutput  = EthereumSigningOutput;
    type TransactionPlan = EthereumTransactionPlan; // gas estimate

    fn new(&self, input: &Self::SigningInput, plan: &Self::TransactionPlan)
        -> Result<EthereumUnsignedTx, Error>
    {
        // Build EIP-1559 or legacy transaction from nonce, gas, to,
        // value, data, chain_id.
    }

    fn prove(&self, bundle: EthereumUnsignedTx, _: &()) -> Result<EthereumUnsignedTx, Error> {
        Ok(bundle) // No proofs.
    }

    fn authorize(&self, bundle: EthereumUnsignedTx, key: &secp256k1::SecretKey)
        -> Result<EthereumUnsignedTx, Error>
    {
        // RLP-encode the unsigned transaction.
        // Keccak-256 hash the encoding.
        // Sign with secp256k1, producing (v, r, s).
        // Attach signature to the bundle.
    }

    fn extract(&self, bundle: EthereumUnsignedTx) -> Result<Self::SigningOutput, Error> {
        // Already signed after authorize.
        // RLP-encode the signed transaction.
    }
}
```

**Key types.** `derive_keys` runs BIP-32 on path `m/44'/60'/account'/0/0`.
Ethereum uses one address per account — `diversifier_index` is ignored in
`derive_address`. `ProofKey` is `()`.

---

## 6. Implementation: Zcash Transparent

Zcash transparent is Bitcoin's UTXO model with additional transaction envelope
fields (`version_group_id`, `branch_id`, `expiry_height`). In the
`librustzcash` stack, transparent functionality lives in the `zcash_transparent`
crate, with key management via `zcash_keys`.

```rust
impl Protocol for ZcashTransparent {
    type Bundle        = pczt::Pczt;
    type ProofKey      = ();
    type SigningKey     = secp256k1::SecretKey;
    type SigningInput   = ZcashTransparentSigningInput;
    type SigningOutput  = ZcashSigningOutput;
    type TransactionPlan = ZcashTransparentPlan;

    fn new(&self, input: &Self::SigningInput, plan: &Self::TransactionPlan)
        -> Result<pczt::Pczt, Error>
    {
        // Use zcash_primitives::transaction::builder::Builder
        //   to add transparent inputs and outputs.
        // Call Builder::build_for_pczt() to produce the PCZT.
        // The pczt crate's "zcp-builder" feature enables this path.
        //
        // Populate PCZT transparent fields:
        //   - per-input: prevout, script_pubkey, value, sequence
        //   - per-output: script_pubkey, value
    }

    fn prove(&self, bundle: pczt::Pczt, _: &()) -> Result<pczt::Pczt, Error> {
        Ok(bundle) // No proofs for transparent.
    }

    fn authorize(&self, bundle: pczt::Pczt, key: &secp256k1::SecretKey)
        -> Result<pczt::Pczt, Error>
    {
        // Use pczt::roles::signer with the transparent feature.
        // For each transparent input:
        //   Compute the sighash per ZIP-244 (TxId digest).
        //   Sign with ECDSA on secp256k1.
        //   Insert script_sig into the PCZT transparent input.
    }

    fn extract(&self, bundle: pczt::Pczt) -> Result<Self::SigningOutput, Error> {
        // Use pczt::roles::tx_extractor.
        // Verify all transparent inputs are signed.
        // Emit serialized v5 transaction bytes.
    }
}
```

**Crate mapping.**

| Trait concept      | `librustzcash` crate               |
| ------------------ | ---------------------------------- |
| `Bundle`           | `pczt::Pczt`                       |
| Key derivation     | `zcash_keys` (transparent feature) |
| UTXO types         | `zcash_transparent`                |
| Tx builder         | `zcash_primitives::transaction`    |
| Signing role       | `pczt::roles::signer`              |
| Extraction role    | `pczt::roles::tx_extractor`        |

---

## 7. Implementation: Zcash Sapling

Sapling introduces the first shielded pool. The key hierarchy is
fundamentally different from BIP-32: ZIP-32 derives a Sapling
`ExtendedSpendingKey` from the seed, which yields a `FullViewingKey` (for
proof generation), a `SpendAuthorizingKey` (for signing), and an
`IncomingViewingKey` (for address derivation). Addresses are
`(diversifier, pk_d)` pairs derived from the IVK — not from a public key
hash.

The proof system is Groth16 (via `zcash_proofs::prover`), requiring
Sapling-specific parameters loaded at runtime.

```rust
impl Protocol for ZcashSapling {
    type Bundle        = pczt::Pczt;
    type ProofKey      = sapling::keys::FullViewingKey;
    type SigningKey     = sapling::keys::SpendAuthorizingKey;
    type SigningInput   = ZcashSaplingSigningInput;
    type SigningOutput  = ZcashSigningOutput;
    type TransactionPlan = ZcashSaplingPlan;

    fn derive_keys(&self, seed: &[u8], account: u32)
        -> Result<KeySet<Self>, Error>
    {
        // ZIP-32 Sapling key derivation:
        //   sapling::zip32::ExtendedSpendingKey::master(seed)
        //     .derive_child(ChildIndex::Hardened(32)) // purpose
        //     .derive_child(ChildIndex::Hardened(133)) // coin type
        //     .derive_child(ChildIndex::Hardened(account))
        //
        // From ExtendedSpendingKey derive:
        //   - SpendAuthorizingKey (ask)     → SigningKey
        //   - FullViewingKey (ak, nk, ovk)  → ProofKey
        //   - IncomingViewingKey (ivk)       → for address derivation
    }

    fn derive_address(&self, keys: &KeySet<Self>, diversifier_index: u32)
        -> Result<Address, Error>
    {
        // From the Sapling IVK (inside AddressKey):
        //   Try diversifier indices starting at diversifier_index
        //   until a valid diversifier is found (not all indices
        //   produce valid diversifiers on the Jubjub curve).
        //   Compute pk_d = ivk * G_d(diversifier).
        //   Encode as Bech32 with "zs" HRP.
    }

    fn new(&self, input: &Self::SigningInput, plan: &Self::TransactionPlan)
        -> Result<pczt::Pczt, Error>
    {
        // Use zcash_primitives::transaction::builder::Builder.
        // For each Sapling spend:
        //   Add note, merkle path, anchor.
        // For each Sapling output:
        //   Add recipient address, value, memo.
        // Call Builder::build_for_pczt().
    }

    fn prove(&self, bundle: pczt::Pczt, fvk: &sapling::keys::FullViewingKey)
        -> Result<pczt::Pczt, Error>
    {
        // Use pczt::roles::prover with the sapling feature.
        //
        // For each Sapling Spend description in the PCZT:
        //   Build the Spend circuit witness (value commitment,
        //     anchor, nullifier, rk, merkle path).
        //   Generate a Groth16 proof using zcash_proofs::prover.
        //   Insert zkproof into the PCZT Sapling spend.
        //
        // For each Sapling Output description:
        //   Build the Output circuit witness.
        //   Generate a Groth16 proof.
        //   Insert zkproof into the PCZT Sapling output.
        //
        // This is computationally expensive (~2s per proof on
        // desktop) and requires the Sapling proving parameters
        // (sapling-spend.params, sapling-output.params).
    }

    fn authorize(&self, bundle: pczt::Pczt, ask: &sapling::keys::SpendAuthorizingKey)
        -> Result<pczt::Pczt, Error>
    {
        // Use pczt::roles::signer with the sapling feature.
        //
        // Compute the PCZT sighash (ZIP-244 transaction digest).
        // For each Sapling Spend:
        //   Generate a RedJubjub spend authorization signature
        //     using ask and the randomized re-randomization alpha.
        //   Insert spend_auth_sig into the PCZT Sapling spend.
    }

    fn extract(&self, bundle: pczt::Pczt) -> Result<Self::SigningOutput, Error> {
        // Use pczt::roles::tx_extractor.
        //
        // Verify all Sapling spends have proofs and signatures.
        // Compute the Sapling binding signature from the accumulated
        //   value commitment randomness (bsk → bvk).
        // Emit serialized v5 transaction.
    }
}
```

**Crate mapping.**

| Trait concept      | `librustzcash` crate / type                    |
| ------------------ | ----------------------------------------------- |
| `Bundle`           | `pczt::Pczt`                                    |
| `ProofKey`         | `sapling::keys::FullViewingKey`                 |
| `SigningKey`        | `sapling::keys::SpendAuthorizingKey`            |
| Key derivation     | `sapling::zip32::ExtendedSpendingKey`           |
| Address derivation | `sapling::keys::IncomingViewingKey`             |
| Prover role        | `pczt::roles::prover` + `zcash_proofs::prover`  |
| Signer role        | `pczt::roles::signer`                           |
| Extractor role     | `pczt::roles::tx_extractor`                     |
| Proof system       | Groth16 (bellman)                               |
| Signature scheme   | RedJubjub (`redjubjub` crate)                   |

---

## 8. Implementation: Zcash Orchard

Orchard is the most complex case and the one that drove the trait's design.
It uses the Pallas curve, RedPallas signatures, and Halo 2 proofs (no trusted
setup). The key hierarchy is defined by ZIP-32 Orchard derivation, entirely
separate from both BIP-32 and ZIP-32 Sapling.

### 8.1. Key Hierarchy

Orchard's key tree, as implemented in the `orchard` crate:

```
orchard::keys::SpendingKey               (derived from seed via ZIP-32)
 ├── orchard::keys::SpendAuthorizingKey  (ask: Pallas scalar)
 ├── SpendValidatingKey                  (ak: Pallas point)
 ├── NullifierDerivingKey                (nk: Pallas point)
 └── orchard::keys::FullViewingKey       (ak, nk, rivk)
      └── orchard::keys::IncomingViewingKey (ivk)
           └── diversifier d → orchard::Address(d, pk_d)
```

Mapping to the trait's `KeySet`:

- `ProofKey` = `orchard::keys::FullViewingKey` — needed during proof
  generation to compute nullifiers, value commitments, and note encryption.
- `SigningKey` = `orchard::keys::SpendAuthorizingKey` — the RedPallas
  scalar used to produce spend authorization signatures.
- `AddressKey` = derived from the `IncomingViewingKey` — used with a
  diversifier index to generate unlinkable payment addresses.

### 8.2. The Bundle's Authorization Typestate

The `orchard` crate uses a compile-time typestate pattern for its `Bundle`
type, parameterized by an `Authorization` associated type. This enforces
correctness of the construction pipeline at the type level:

```
Bundle<InProgress<Unproven, Unauthorized>>    // after new
  → Bundle<InProgress<Proven, Unauthorized>>  // after prove
  → Bundle<InProgress<Proven, PartiallyAuthorized>> // after authorize (partial)
  → Bundle<Authorized>                        // fully signed, extractable
```

The `pczt::Pczt` type wraps this progression in a serializable format. The
PCZT itself does not carry Rust type-level authorization state — instead, it
carries flags and optional fields that the `pczt::roles::*` modules validate
at each step. The Orchard typestate re-emerges inside the extraction step
when the PCZT is converted back into an `orchard::Bundle<Authorized>`.

### 8.3. Implementation

```rust
impl Protocol for ZcashOrchard {
    type Bundle        = pczt::Pczt;
    type ProofKey      = orchard::keys::FullViewingKey;
    type SigningKey     = orchard::keys::SpendAuthorizingKey;
    type SigningInput   = ZcashOrchardSigningInput;
    type SigningOutput  = ZcashSigningOutput;
    type TransactionPlan = ZcashOrchardPlan;

    fn derive_keys(&self, seed: &[u8], account: u32)
        -> Result<KeySet<Self>, Error>
    {
        // ZIP-32 Orchard key derivation:
        //   orchard::keys::SpendingKey::from_zip32_seed(
        //       seed, coin_type, account
        //   )
        //
        // From SpendingKey:
        //   .into() → FullViewingKey            (ProofKey)
        //   SpendAuthorizingKey::from(&sk)       (SigningKey)
        //   FullViewingKey → IncomingViewingKey  (for addresses)
    }

    fn derive_address(&self, keys: &KeySet<Self>, diversifier_index: u32)
        -> Result<Address, Error>
    {
        // orchard::keys::FullViewingKey::address_at(
        //     diversifier_index, Scope::External
        // )
        //
        // Unlike Sapling, every Orchard diversifier index is valid.
        // The address is (diversifier, pk_d) on the Pallas curve.
        // Encoded as part of a Unified Address (ZIP-316) with "u" HRP.
    }

    fn new(&self, input: &Self::SigningInput, plan: &Self::TransactionPlan)
        -> Result<pczt::Pczt, Error>
    {
        // Use zcash_primitives::transaction::builder::Builder.
        //
        // For each Orchard Action (merged spend + output):
        //   Add note to spend (with merkle path from shardtree).
        //   Add recipient (address, value, memo).
        //   Pad with dummy actions to hide real spend/output count.
        //
        // Call Builder::build_for_pczt().
        //
        // The resulting PCZT contains:
        //   pczt::orchard::Action entries with:
        //     - spend: nullifier, rk, spend_auth_sig (empty)
        //     - output: cmx, encrypted note, ephemeral key
        //     - cv_net (value commitment)
        //     - zkproof (empty, to be filled by prover)
    }

    fn prove(&self, bundle: pczt::Pczt, fvk: &orchard::keys::FullViewingKey)
        -> Result<pczt::Pczt, Error>
    {
        // Use pczt::roles::prover with the orchard feature.
        //
        // For each Orchard Action in the PCZT:
        //   Reconstruct the circuit witness from the PCZT fields
        //     and the FullViewingKey.
        //   Generate a Halo 2 proof using:
        //     orchard::circuit::ProvingKey::build()
        //     (no trusted setup — the proving key is deterministic)
        //   Insert the proof into the PCZT action's zkproof field.
        //
        // This is the computationally expensive step.
        // On desktop hardware: ~1-3s per action.
        // On a hardware wallet: impossible (insufficient memory).
        // This is WHY prove and authorize are separate roles.
    }

    fn authorize(&self, bundle: pczt::Pczt, ask: &orchard::keys::SpendAuthorizingKey)
        -> Result<pczt::Pczt, Error>
    {
        // Use pczt::roles::signer with the orchard feature.
        //
        // Compute the PCZT transaction sighash (ZIP-244 digest).
        //
        // For each Orchard Action:
        //   Compute the randomized spend authorizing key:
        //     rsk = ask + alpha (where alpha is the action's
        //     randomizer, stored in the PCZT).
        //   Produce a RedPallas signature over the sighash using rsk.
        //   Insert spend_auth_sig into the PCZT action.
        //
        // This step is lightweight — just a scalar multiply and
        // a signature. It CAN run on a hardware wallet.
    }

    fn extract(&self, bundle: pczt::Pczt) -> Result<Self::SigningOutput, Error> {
        // Use pczt::roles::tx_extractor with the orchard feature.
        //
        // Verify all actions have both zkproof and spend_auth_sig.
        // Verify proofs against:
        //   orchard::circuit::VerifyingKey::build()
        //
        // Compute the Orchard binding signature:
        //   bsk = sum of all value commitment randomness (rcv)
        //   Sign the sighash with bsk to produce binding_sig.
        //   This ties the value balance of the entire bundle.
        //
        // Strip PCZT metadata.
        // Emit serialized v5 transaction bytes.
    }
}
```

### 8.4. The Hardware Wallet Flow

The role separation enables the following device-crossing flow for Orchard:

```
 Desktop / Cloud                  Hardware Wallet (Ledger/Keystone)
 ────────────────                 ─────────────────────────────────

 new(input, plan)
       │
       ▼
    Pczt (no proofs, no sigs)
       │
 prove(pczt, fvk)
       │
       ▼
    Pczt (proofs present,         serialize & transmit
          no sigs)          ─────────────────────────►
                                  authorize(pczt, ask)
                                        │
                                        ▼
                                  Pczt (proofs + sigs)
                            ◄─────────────────────────
       │
 extract(pczt)
       │
       ▼
 Raw v5 transaction
       │
 broadcast
```

The critical insight: `prove` runs on a machine with gigabytes of RAM and
multi-second compute budget. `authorize` runs on a constrained device that
can only do scalar arithmetic on the Pallas curve. The PCZT serialization
format bridges them.

---

## 9. Unified Zcash Transactions

Real Zcash transactions may contain transparent, Sapling, and Orchard
components simultaneously. The `pczt::Pczt` struct carries all three as
optional bundles. A unified Zcash implementation would compose the three
protocol implementations:

```rust
impl Protocol for Zcash {
    type Bundle        = pczt::Pczt;
    type ProofKey      = UnifiedProofKey;       // ((), Option<SaplingFVK>, Option<OrchardFVK>)
    type SigningKey     = UnifiedSigningKey;     // (Option<secp256k1::SecretKey>,
                                                //  Option<SaplingASK>,
                                                //  Option<OrchardASK>)
    type SigningInput   = ZcashSigningInput;     // may contain all three pools
    type SigningOutput  = ZcashSigningOutput;
    type TransactionPlan = ZcashTransactionPlan;
```

This mirrors `zcash_keys::keys::UnifiedSpendingKey`, which bundles
transparent, Sapling, and Orchard spending keys derived from a single seed
via ZIP-32. `derive_keys` calls `UnifiedSpendingKey::from_seed()`, and each
role method delegates to the per-pool logic internally.

The `derive_address` method produces a **Unified Address** (ZIP-316) — a
single address string (HRP `"u"`) that encodes receivers for all three pools.
A sender's wallet picks the most shielded receiver it supports. Internally,
the implementation calls `derive_address` for each pool at the same
diversifier index, then assembles them into a
`zcash_keys::address::UnifiedAddress`.

---

## 10. Crate Dependency Map

For a multichain wallet integrating Bitcoin, Ethereum, and Zcash via the
`Protocol` trait:

```
your-wallet
 ├── bitcoin          (PSBT, tx building, script)
 ├── secp256k1        (shared by Bitcoin, Ethereum, Zcash transparent)
 │
 ├── [your Ethereum tx types]
 │
 ├── pczt             (PCZT format, role implementations)
 │    ├── orchard     (Orchard protocol types, Halo 2 circuit)
 │    ├── sapling-crypto (Sapling protocol types)
 │    └── zcash_transparent
 │
 ├── zcash_primitives (transaction builder, consensus rules)
 ├── zcash_proofs     (Groth16 prover for Sapling)
 ├── zcash_keys       (UnifiedSpendingKey, address encoding)
 ├── zcash_protocol   (NetworkConstants, consensus parameters)
 │
 └── zip32            (ZIP-32 key derivation for Sapling + Orchard)
```

---

## 11. What the Trait Does Not Cover

**Block scanning and note discovery.** The trait covers transaction
construction and signing. Discovering which notes belong to a wallet (trial
decryption of the blockchain) is handled by `zcash_client_backend`'s scanning
infrastructure and is orthogonal to signing.

**Fee policy.** The `plan` method returns a `TransactionPlan`, but the fee
algorithm (ZIP-317 for Zcash, EIP-1559 for Ethereum, fee-rate estimation for
Bitcoin) is internal to each implementation. The trait does not prescribe a
fee model.

**Network interaction.** Broadcasting, mempool management, and confirmation
tracking are transport-layer concerns outside the trait.

**View-only wallets.** The trait is signing-oriented. A view-only wallet
would use `derive_address` and the scanning infrastructure but would never
call `prove`, `authorize`, or `sign`.
