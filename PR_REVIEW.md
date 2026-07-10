# PR Review: `add-mobile-signer` → `add-mobile-signer-redesign-signer`

**11 commits**, 57 files changed, +9369 / -3762 lines.

---

## Critical Issues

## MAJOR ISSUE

Refactor has been botched.

It is essential we reuse Keypunk from `./keypunkd` in the signer tauri app found in `./signer`.

We should plug in the appropriate seed store for android.



### 1. Hardcoded seed phrase in Tauri signer

**File:** `signer/src-tauri/src/signer.rs:32-37`

`SignerState::create()` embeds a literal mnemonic (`"ribbon velvet ocean puzzle harvest guitar shadow ladder comfort raven spring anchor"`). Every instance of the app shares the same wallet. The `generate_seed` Tauri command just calls `create()` again — it doesn't actually generate a new seed. There is no persistence; the seed is lost on restart.

```rust
pub fn create() -> Self {
    let mnemonic =
        "ribbon velvet ocean puzzle harvest guitar shadow ladder comfort raven spring anchor"
            .to_string();
    let seed = bip39::Mnemonic::parse(&mnemonic)
        .expect("valid mnemonic")
        .to_seed("");
```

### 2. Zcash TUI review shows placeholders instead of real data

**File:** `tui/src/api/real.rs:466-477`

When mapping `ArtifactSummary::Zcash` to `SendReviewData`, `to_address` is hardcoded to `"Zcash transfer"` and `amount` to `"0"`. The `OutputEntry` data (real recipient addresses and amounts) from the summary is ignored. Users cannot see what they're sending or to whom.

```rust
ArtifactSummary::Zcash(zcash) => {
    let total = zcash.fee.parse::<u128>().unwrap_or(0);
    SendReviewData {
        to_address: "Zcash transfer".to_string(),  // placeholder
        amount: "0".to_string(),                    // always zero
        ...
```

---

## High Priority

### 3. f64 amount parsing loses precision

**File:** `protocols/zcash/src/protocol.rs:83-84`

ZEC amounts are parsed via `amount.parse::<f64>() * 100_000_000.0 as u64`. This loses precision above ~90,071 ZEC (2^53 zatoshis). Should use decimal string parsing with integer arithmetic instead.

```rust
let amount_f64: f64 = amount.parse().map_err(|_| "invalid amount".to_string())?;
let amount_zat = (amount_f64 * 100_000_000.0) as u64;
```

### 4. Signer over-signs when zip32 metadata is missing

**File:** `protocols/zcash/src/signer.rs:127-135`

When no zip32 derivation info is found in the PCZT, the signer falls back to signing **every** Orchard action with the single derived `ask`. This signs actions belonging to other accounts. Errors from `sign_orchard` are silently discarded with `let _ =`.

```rust
if keys.is_empty() {
    let num_actions = pczt.orchard().actions().len();
    let mut signer = Signer::new(pczt)...;
    for i in 0..num_actions {
        let _ = signer.sign_orchard(i, &ask);
    }
```

### 5. Multi-network signer caching bug

**File:** `signer/src-tauri/src/signer.rs:47-58`

The Zcash signer is initialized once based on the first `chain_id` seen. Processing artifacts for different networks (mainnet + testnet) silently uses wrong params. The signer should be keyed by network, not lazily initialized once.

### 6. Signer mode bypasses all authentication

No password is required to send funds, no review screen is shown to the user, and the background task atomically submits + broadcasts. Acceptable for air-gapped QR use but dangerous if the bridge is on the same machine — anyone with access to the running daemon can send funds without authentication.

---

## Medium Priority

### 7. Ethereum signer recovery ID brute-force

**File:** `protocols/ethereum/src/signer.rs:85-94`

Tries both `v=0` and `v=1` recovery IDs instead of computing it directly from the signature's `y_is_odd` flag. Correct but unnecessarily roundabout.

```rust
let rec_id = [0u8, 1]
    .into_iter()
    .find_map(|id| {
        let rid = RecoveryId::from_byte(id)?;
        VerifyingKey::recover_from_prehash(sighash.as_ref(), &sig, rid)
            .ok()
            .filter(|recovered| recovered == signing_key.verifying_key())
            .map(|_| rid)
    })
```

### 8. Broadcast errors discarded in signer mode

**File:** `tui/src/api/real.rs:602`

The signer mode background task's broadcast failure error is captured as `_e` and replaced with `"failed"`. The actual error message is lost.

```rust
Ok(Err(_e)) => {
    *guard = None;
    return Some(SendResult {
        tx_hash: String::new(),
        status: "failed".to_string(),  // _e is lost
        ...
    });
}
```

### 9. ProvingKey rebuilt on every sign() call

**File:** `protocols/zcash/src/signer.rs:119`

`ProvingKey::build()` constructs the proving key from Orchard parameters on every `sign()` call. This is expensive and should be cached in the `ZcashSignerProtocol` struct.

```rust
let orchard_pk = ProvingKey::build();
```

### 10. `write_default` uses unexpanded `~` paths

**File:** `config/src/lib.rs:145-146`

The config template writes `~/.local/share/paypunk/` literally — `~` is not expanded by Rust filesystem APIs. The `default_data_dir()` and `default_config_dir()` functions return proper absolute paths via `dirs::data_dir()` but the written config file will have broken paths.

### 11. Dead code in paypunkd

**File:** `paypunkd/src/paypunkd.rs:108-136`

The `ArtifactAuthorized` match arm in `submit_intent` is unreachable — that response only comes from `authorize_artifact`, not `preview_artifact`.

### 12. `get_signer_status` can panic

**File:** `signer/src-tauri/src/lib.rs:19`

Uses `lock().unwrap()` instead of error propagation like the other commands.

```rust
fn get_signer_status(state: State<AppState>) -> String {
    let signer = state.signer.lock().unwrap();
```

---

## Low Priority

| Issue | File | Detail |
|-------|------|--------|
| Unused hash computation | `cli/src/main.rs:738-742` | `_hash = Blake2b::digest(...)` computed but never used |
| `chain_id` inconsistency | `keypunkd/src/keypunk.rs:361` | Discarded by keypunkd but used by Tauri signer for network config |
| Tauri as workspace member | `Cargo.toml` | Requires webkit2gtk etc. for basic `cargo build` — should be optional/feature-gated |
| Coin type validation | `protocols/zcash/src/common.rs:11-20` | `account_from_path` doesn't validate coin type 133 — Ethereum paths silently extract wrong account |
| `#[async_trait(?Send)]` inconsistency | `tui/src/screens/send.rs:131` | Uses `?Send` while API trait now requires `Send` |
