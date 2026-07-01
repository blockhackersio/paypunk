# Step 4: Persist settings and address book to SQLite

## Issue
**#1** — Address book entries are stored in an in-memory `Mutex<Vec<AddressBookEntry>>` and lost on restart. No SQLite table or IPC messages exist.
**#3** — `submit_settings()` accepts changes but never persists them. No IPC or DB write occurs.
**#11** — `get_settings()` always returns hardcoded values. Never reads from config file or DB.

## What to do

1. **Add DB migrations** in `paypunkd/src/database/migration.rs`:
   - Migration v4: `address_book` table with columns: `id INTEGER PRIMARY KEY AUTOINCREMENT`, `name TEXT NOT NULL`, `address TEXT NOT NULL UNIQUE`, `protocol TEXT NOT NULL`, `created_at INTEGER NOT NULL`
   - Migration v5: `settings` table with columns: `key TEXT PRIMARY KEY`, `value TEXT NOT NULL`

2. **Add IPC messages** in `paypunkd/src/messages.rs`:
   - `GetAddressBook` / response `AddressBookData { entries: Vec<AddressBookEntry> }`
   - `AddAddressBookEntry { name: String, address: String, protocol: String }` / response `AddressBookEntryAdded`
   - `GetSettings` / response `SettingsResult { auto_lock_minutes: u32, fiat_currency: String }`
   - `SaveSettings { auto_lock_minutes: u32, fiat_currency: String }` / response `SettingsSaved`

3. **Add usecases** in `paypunkd/src/usecases.rs`:
   - `get_address_book(db, repo) -> Vec<AddressBookEntry>`
   - `add_address_book_entry(db, name, address, protocol) -> Result<()>`
   - `get_settings(db) -> (u32, String)`
   - `save_settings(db, auto_lock_minutes, fiat_currency) -> Result<()>`

4. **Add handlers** in `paypunkd/src/paypunkd.rs` for the new messages.

5. **Add repository trait** for address book entries (similar to `AccountsRepository`).

6. **Wire RealWalletApi** (`tui/src/api/real.rs`):
   - `get_address_book()` — call IPC instead of reading in-memory vec
   - `add_address_book_entry()` — call IPC instead of pushing to in-memory vec
   - `get_settings()` — call IPC instead of returning hardcoded values
   - `submit_settings()` — call IPC instead of no-op

7. **Add IPC methods to `PaypunkService`** (`paypunkd/src/services.rs`) and **api `Client`** (`api/src/client.rs`, `api/src/functions.rs`).

## Verification
- `cargo build` succeeds
- `cargo test` passes
- Address book entries survive daemon restart (persisted to SQLite)
- Settings changes persist and are returned correctly after restart
- Empty address book returns empty list (no crash)
