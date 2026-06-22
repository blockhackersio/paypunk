# Step 1: Add `address` field to Account struct

## Context

The `Account` struct in `paypunk-types` needs an `address` field so the TUI can display addresses without additional RPC calls. This field will be populated during account creation by deriving the address from the viewing key.

## Changes

### `types/src/lib.rs`
- Add `pub address: String` field to the `Account` struct (between `name` and `viewing_key`)
- Add `pub nonce: u64` field to the `ArtifactSummary` struct (between `fee` and `memo`)

### `paypunkd/src/database/migration.rs`
- Add a new `AddAddressToAccounts` migration (version 3):
  ```sql
  ALTER TABLE accounts ADD COLUMN address TEXT NOT NULL DEFAULT '';
  ```
- Register it in `Database::run_migrations()` after `AccountsMigration`

### `paypunkd/src/database/repository.rs`
- Update `save()` SQL to include `address` column: `INSERT INTO accounts (id, protocol, derivation_path, name, address, viewing_key, created_at)`
- Update `find_all()` SELECT to include `address`
- Update `find_by_id()` SELECT to include `address`
- Update `find_by_protocol()` SELECT to include `address`

### `paypunkd/src/usecases.rs`
- In `create_account()`: set `address: String::new()` (temporary — will be populated in Step 3)
- In `bulk_derive_accounts()`: set `address: String::new()` (temporary)

## Acceptance Criteria

- [ ] `Account` struct has `address: String` field
- [ ] Database schema includes `address` column
- [ ] Repository reads/writes the `address` column
- [ ] `cargo build` succeeds across the workspace
- [ ] `cargo test` passes

## Tests

- Existing `test_db_create_and_migrate` should still pass (migration v3 runs)
- Existing `test_db_reopen_reads_data` should still pass (address column populated with '')
- Run `cargo test` in workspace
