# Step 3: Zcash protocol split — common.rs, feature gates, ZcashSignerProtocol

## Goal

Create `protocols/zcash/src/common.rs` with shared helpers. Feature-gate wallet
deps in `protocols/zcash/Cargo.toml`. Create `protocols/zcash/src/signer.rs` with
`ZcashSignerProtocol` implementing `SignerProtocol`. `ZcashProtocol` still has its
own `SignerProtocol` impl — nothing is removed. This stage is purely additive.

## Files to change

### 1. `protocols/zcash/src/common.rs` — New file

Create with these shared helpers:

```rust
use orchard::Address;
use zcash_address::unified;
use zcash_address::{ToAddress, ZcashAddress};
use zcash_protocol::consensus::NetworkType;

pub const ZCASH_COIN_TYPE: u32 = 133;

/// Extract the account index from a BIP44 derivation path.
/// Path format: "m/44'/133'/{account}'"
pub fn account_from_path(path: &str) -> Result<u32, String> {
    let account_str = path
        .rsplit('\'')
        .nth(1)
        .and_then(|s| s.split('/').last())
        .ok_or_else(|| format!("invalid derivation path: {path}"))?;
    account_str
        .parse()
        .map_err(|_| format!("invalid account index in path: {path}"))
}

/// Decode a raw Orchard address ([u8; 43]) to a human-readable unified address.
pub fn decode_orchard_recipient(raw: &[u8; 43], net: NetworkType) -> Option<String> {
    let orchard_addr = Address::from_raw_address_bytes(raw).into_option()?;
    let raw = orchard_addr.to_raw_address_bytes();
    let ua = unified::Address::try_from_items(vec![unified::Receiver::Orchard(raw)]).ok()?;
    let zaddr = ZcashAddress::from_unified(net, ua);
    Some(zaddr.encode())
}
```

Dependencies needed: `orchard`, `zcash_address`, `zcash_protocol` — these are
already in `Cargo.toml`.

### 2. `protocols/zcash/Cargo.toml`

Expand the `[features]` section. Currently:

```toml
[features]
default = ["wallet"]
wallet = ["dep:tactix", "dep:tokio"]
```

Change to:

```toml
[features]
default = ["wallet"]
wallet = [
    "dep:tactix",
    "dep:tokio",
    "dep:zcash_client_backend",
    "dep:zcash_client_sqlite",
    "dep:rusqlite",
    "dep:tonic",
    "dep:reqwest",
    "dep:serde_json",
    "dep:secrecy",
]
```

In the `[dependencies]` section, mark these deps as `optional = true`:

```toml
tactix = { workspace = true, optional = true }
tokio = { workspace = true, optional = true }
zcash_client_backend = { workspace = true, optional = true }
zcash_client_sqlite = { workspace = true, optional = true }
rusqlite = { workspace = true, optional = true }
tonic = { workspace = true, optional = true }
reqwest = { workspace = true, optional = true }
serde_json = { workspace = true, optional = true }
secrecy = { workspace = true, optional = true }
```

`tracing` and `thiserror` remain always available (not optional).

Verify the build with `--no-default-features`:

```bash
cargo build -p paypunk-chains-zcash --no-default-features
```

### 3. `protocols/zcash/src/signer.rs` — New file

Create `ZcashSignerProtocol` with `impl SignerProtocol`. This struct only needs
`params` and `network_type` (no wallet DB, no scan actor, no lightwalletd host).

```rust
use async_trait::async_trait;
use bip32::secp256k1::rand::rngs::OsRng;
use orchard::keys::{FullViewingKey, SpendAuthorizingKey, SpendingKey};
use paypunk_types::{
    ArtifactSummary, OutputEntry, ProtocolId, SignerProtocol, ZcashArtifactSummary,
};
use pczt::roles::{Prover, Verifier};
use pczt::Pczt;
use rand::rngs::OsRng as _;
use zcash_keys::keys::UnifiedSpendingKey;
use zcash_primitives::consensus::LocalNetwork;
use zcash_protocol::consensus::NetworkType;
use zip32::ChildIndex;

use crate::common::{account_from_path, decode_orchard_recipient, ZCASH_COIN_TYPE};

pub struct ZcashSignerProtocol {
    pub params: LocalNetwork,
    network_type: NetworkType,
}

impl ZcashSignerProtocol {
    pub fn new(params: LocalNetwork, network_type: NetworkType) -> Self {
        Self { params, network_type }
    }
}

#[async_trait]
impl SignerProtocol for ZcashSignerProtocol {
    fn export_viewing(&self, seed: &[u8; 64], path: &str) -> Result<Vec<u8>, String> {
        let account = account_from_path(path)?;
        let usk = UnifiedSpendingKey::from_seed(
            &self.params,
            seed,
            account,
        )
        .map_err(|e| format!("derive USK failed: {e}"))?;
        let fvk = usk.to_unified_full_viewing_key();
        let orchard_fvk = fvk.orchard().ok_or("no orchard key")?;
        Ok(orchard_fvk.to_bytes().to_vec())
    }

    fn parse_artifact(&self, artifact: &[u8]) -> Result<Vec<u8>, String> {
        let pczt = Pczt::parse(artifact).map_err(|e| format!("PCZT parse failed: {e:?}"))?;

        let (value_sum, negative) = pczt.orchard().value_sum();
        let fee = if *negative { 0u64 } else { *value_sum };

        let mut outputs = Vec::new();
        for action in pczt.orchard().actions() {
            if let (Some(recipient_raw), Some(value)) =
                (action.output().recipient(), action.output().value())
            {
                if let Some(addr) = decode_orchard_recipient(recipient_raw, self.network_type) {
                    outputs.push(OutputEntry { address: addr, amount: value.to_string() });
                }
            }
        }

        let summary = ArtifactSummary::Zcash(ZcashArtifactSummary {
            outputs,
            fee: fee.to_string(),
        });

        postcard::to_allocvec(&summary).map_err(|e| format!("serialize summary failed: {e}"))
    }

    fn sign(&self, seed: &[u8; 64], path: &str, artifact: &[u8]) -> Result<Vec<u8>, String> {
        let account = account_from_path(path)?;
        self.sign_transaction_inner(seed, account, artifact)
    }
}

impl ZcashSignerProtocol {
    fn sign_transaction_inner(
        &self,
        seed: &[u8; 64],
        account: u32,
        transaction: &[u8],
    ) -> Result<Vec<u8>, String> {
        let mut pczt = Pczt::parse(transaction).map_err(|e| format!("PCZT parse failed: {e:?}"))?;

        let usk = UnifiedSpendingKey::from_seed(&self.params, seed, account)
            .map_err(|e| format!("derive USK failed: {e}"))?;

        let mut rng = OsRng;

        let mut proven = pczt
            .prove_orchard::<Prover>(&self.params, &mut rng)
            .map_err(|e| format!("orchard proving failed: {e:?}"))?;

        let verifier = proven
            .verify_orchard::<Verifier>(&self.params)
            .map_err(|e| format!("orchard verification failed: {e:?}"))?;

        let ask = SpendAuthorizingKey::from(
            usk.orchard()
                .ok_or("no orchard spending key")?
                .spending_key(),
        );
        let keys: Vec<KeyRef> = verifier
            .keys()
            .iter()
            .enumerate()
            .filter_map(|(i, key)| {
                if key.0 == account {
                    Some(KeyRef::Orchard { index: i })
                } else {
                    None
                }
            })
            .collect();

        if keys.is_empty() {
            for i in 0..proven.orchard().actions().len() {
                proven
                    .sign_orchard(i, &ask, &mut rng)
                    .map_err(|e| format!("orchard signing failed: {e:?}"))?;
            }
        } else {
            for key in &keys {
                match key {
                    KeyRef::Orchard { index } => {
                        proven
                            .sign_orchard(*index, &ask, &mut rng)
                            .map_err(|e| format!("orchard signing failed: {e:?}"))?;
                    }
                }
            }
        }

        proven.to_bytes().map_err(|e| format!("PCZT serialization failed: {e:?}"))
    }
}

enum KeyRef {
    Orchard { index: usize },
}
```

### 4. `protocols/zcash/src/lib.rs`

Add the new modules and update exports:

```rust
pub mod address;
pub mod common;
pub mod protocol;
pub mod signer;

#[cfg(feature = "wallet")]
pub mod lsp_client;
#[cfg(feature = "wallet")]
pub mod scan_actor;
#[cfg(feature = "wallet")]
pub mod wallet_actor;

pub use protocol::ZcashProtocol;
pub use signer::ZcashSignerProtocol;

#[cfg(feature = "wallet")]
pub use scan_actor::{Sync, SyncNewAccount};
#[cfg(feature = "wallet")]
pub use wallet_actor::{
    EstimateFee, GetBalance, GetBlockHeight, GetChainTip, GetHistory, GetStatus, GetTxStatus,
    ProposeAndBuild, RegisterAccount, ScanBlocks, ScanUpdate, StoreTransaction, WalletDbActor,
};

#[cfg(feature = "wallet")]
pub fn create_protocol(/* ... */) -> ZcashStack { /* ... existing ... */ }

#[cfg(feature = "wallet")]
pub struct ZcashStack { /* ... existing ... */ }

// Always available:
pub fn to_local_params(/* ... */) -> LocalNetwork { /* ... existing ... */ }
pub fn derivation_path(account: u32) -> String { /* ... existing ... */ }
pub fn patch_orchard_views_for_regtest(/* ... */) { /* ... existing ... */ }
#[cfg(feature = "wallet")]
pub fn open_wallet_db(/* ... */) -> Result<WalletDb<...>, ...> { /* ... existing ... */ }
```

### 5. `protocols/zcash/src/protocol.rs`

Update the `export_viewing` method in `impl SignerProtocol for ZcashProtocol` to
use `account_from_path` from `crate::common` instead of its local implementation.
Import `crate::common::account_from_path` and `crate::common::ZCASH_COIN_TYPE`.

Similarly, update `parse_artifact` to use `crate::common::decode_orchard_recipient`
for real address extraction (matching the `ZcashSignerProtocol` implementation).

## Acceptance criteria

1. `cargo build --workspace` succeeds with default features (wallet ON).
2. `cargo build -p paypunk-chains-zcash --no-default-features` succeeds (signer-only).
3. `cargo test --workspace` passes — all tests including pczt_test.rs pass.
4. `cargo fmt --all` produces no changes.
5. Both `ZcashProtocol` and `ZcashSignerProtocol` compile and implement `SignerProtocol`.
6. `common.rs` exists with `account_from_path`, `decode_orchard_recipient`, `ZCASH_COIN_TYPE`.
7. Wallet modules (`lsp_client`, `scan_actor`, `wallet_actor`) are gated behind
   `#[cfg(feature = "wallet")]`.
8. `ZcashSignerProtocol::parse_artifact` extracts real output data from the PCZT
   (recipient addresses and amounts from Orchard actions).

## Context

The `sign_transaction_inner` and `KeyRef` code is duplicated between
`ZcashProtocol::sign_transaction_inner` and `ZcashSignerProtocol::sign_transaction_inner`.
This is intentional — the `ZcashProtocol` version will be removed in Step 5 when
the `SignerProtocol` impl is stripped from `ZcashProtocol`.

The `parse_artifact` in `ZcashSignerProtocol` now extracts real data from the PCZT:
- For each Orchard action, `action.output().recipient()` gives a raw `[u8; 43]`
  which is decoded via `decode_orchard_recipient` to a human-readable unified address.
- `action.output().value()` gives the amount in zatoshis.
- Fee comes from `pczt.orchard().value_sum()`.
- All outputs are listed (recipient + change) — the user verifies the recipient is
  among them.

The `ZcashProtocol::parse_artifact` should also be updated to use real extraction
matching this implementation, so the keypunkd mode also shows real data.

## Verification

```bash
cargo build --workspace
cargo build -p paypunk-chains-zcash --no-default-features
cargo test --workspace
cargo test -p paypunk-chains-zcash
cargo fmt --all
```

After verification, move this file to `./project/done/03_step.md` and commit with:

```
git add -A && git commit -m "zcash: add common.rs helpers, feature-gate wallet deps, add ZcashSignerProtocol"
```