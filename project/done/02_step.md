# Step 2: Add `pre_derived_keys` table migration

## Context

Viewing keys derived during unlock need to be stored in the database (not just in memory) so that `create_account` can create Account records without requiring the password again. This step adds the table; usage comes in Step 3.

## Changes

### `paypunkd/src/database/migration.rs`
- Add a new `PreDerivedKeysMigration` (version 4):
  ```sql
  CREATE TABLE IF NOT EXISTS pre_derived_keys (
      protocol TEXT NOT NULL,
      account_index INTEGER NOT NULL,
      viewing_key BLOB NOT NULL,
      created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
      PRIMARY KEY (protocol, account_index)
  );
  ```
- Register it in `Database::run_migrations()` after `AddAddressToAccounts`

### `paypunkd/src/database/db.rs`
- Register `PreDerivedKeysMigration` in `run_migrations()` method

## Acceptance Criteria

- [ ] `pre_derived_keys` table exists after migration
- [ ] `cargo build` succeeds
- [ ] `cargo test` passes

## Tests

- Run `cargo test` — existing migration tests pass, new migration runs without error
