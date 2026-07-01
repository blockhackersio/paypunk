# Step 1: Types + Config Foundation

## Goal
Add the `SyncStatus` type to the types crate and `lightwalletd_host`/`zcash_network` config fields.

## Changes

### 1. `types/src/lib.rs`

Add after `pub struct PaymentProof(pub Vec<u8>);` (line 234):

```rust
/// Status of a chain sync operation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct SyncStatus {
    pub is_syncing: bool,
    pub current_height: u64,
    pub target_height: u64,
}
```

### 2. `config/src/lib.rs`

Add two new fields to `PaypunkConfig`:

```rust
#[serde(default = "default_lightwalletd_host")]
pub lightwalletd_host: String,
#[serde(default = "default_zcash_network")]
pub zcash_network: String,
```

Add default functions:
```rust
fn default_lightwalletd_host() -> String {
    String::new() // empty = not configured, Zcash won't work
}

fn default_zcash_network() -> String {
    "testnet".to_string()
}
```

Update `Default` impl to include new fields.

Update `apply_env_overrides`:
```rust
if let Ok(v) = std::env::var("PAYPUNK_LIGHTWALLETD_HOST") {
    config.lightwalletd_host = v;
}
if let Ok(v) = std::env::var("PAYPUNK_ZCASH_NETWORK") {
    config.zcash_network = v;
}
```

Update `write_default()` template to include the new fields.

## Verification
- `cargo build -p paypunk-types` succeeds
- `cargo build -p paypunk-config` succeeds
- `cargo test -p paypunk-config` passes
