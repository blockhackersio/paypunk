# Step 14: Cleanup — remove biometric, unused code, dead references

## Context

After all the changes, clean up remaining dead code, biometric references, and unused types/components.

## Changes

### Search and remove all biometric references
- `"face-id-assertion-token"` — should already be gone from Step 11
- `"biometric"` auth_type strings
- Any `LockAuthMethods` biometric-related fields (if they exist)

### Remove unused types from `tui/src/api/types.rs`
- `FeeData`, `FeeDataEth`, `FeeRates`, `UtxoInfo` — if not removed in Step 5
- `PendingTx` — if not removed in Step 5
- `WalletDerivation` — no longer used (WalletsScreen replaced by HomeScreen)

### Remove unused components
- `BalanceItem` — no longer used by HomeScreen
- `WalletItem` — no longer used (WalletsScreen deprecated)
- `AssetItem` — verify still used by AssetsScreen

### Remove unused API methods
- `get_wallets()` — removed from trait in Step 6, remove from both implementations

### Clean up `RealWalletApi`
- Remove `derivation_index` field
- Remove `PendingSend` if it can be simplified (keep if still needed for two-phase auth)

### Verify
- `cargo build` with no warnings
- `cargo test` passes
- No dead code warnings (or add `#[allow(dead_code)]` only where intentional)

## Acceptance Criteria

- [ ] No biometric references anywhere in TUI code
- [ ] No unused types remain in `types.rs`
- [ ] No unused components remain
- [ ] `cargo build` succeeds with no warnings
- [ ] `cargo test` passes
