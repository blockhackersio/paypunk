# Step 4: Remove `pre_derived_keys` HashMap from Paypunkd

## Context

Now that viewing keys are stored in the database (Step 3), the in-memory `pre_derived_keys: HashMap<(ProtocolId, u32), Vec<u8>>` field on the `Paypunkd` struct is redundant. Remove it and all references.

## Changes

### `paypunkd/src/paypunkd.rs`
- Remove `pre_derived_keys` field from `Paypunkd` struct
- Remove `pre_derived_keys` from `Paypunkd::new()` constructor
- Remove the HashMap insert in the `unlock()` handler
- Update `create_account()` handler to pass `&self.db` instead of `&self.pre_derived_keys`

### `paypunkd/src/usecases.rs`
- `create_account()` already updated in Step 3 to take `&Database` instead of `&HashMap`

## Acceptance Criteria

- [ ] `Paypunkd` struct no longer has `pre_derived_keys` field
- [ ] All account creation reads from DB `pre_derived_keys` table
- [ ] `cargo build` succeeds
- [ ] `cargo test` passes
