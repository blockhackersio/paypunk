# Step 3: Modify unlock flow — save viewing keys + create Ethereum Account 0

## Context

During unlock, keypunkd bulk-exports viewing keys for all protocols (indices 0-29). Currently these are cached in an in-memory HashMap. We need to:
1. Save them to the `pre_derived_keys` table
2. Create Ethereum Account 0 automatically (with real address derived from viewing key)

## Changes

### `paypunkd/src/usecases.rs`

**Add `save_pre_derived_key()` helper:**
```rust
pub fn save_pre_derived_key(
    db: &Database,
    protocol: ProtocolId,
    account_index: u32,
    viewing_key: &[u8],
) -> Result<(), String> {
    let conn = db.conn.as_ref().ok_or("database is locked")?;
    let conn = conn.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO pre_derived_keys (protocol, account_index, viewing_key) VALUES (?1, ?2, ?3)",
        rusqlite::params![format!("{:?}", protocol), account_index, viewing_key],
    ).map_err(|e| format!("failed to save pre-derived key: {e}"))?;
    Ok(())
}
```

**Add `get_pre_derived_key()` helper:**
```rust
pub fn get_pre_derived_key(
    db: &Database,
    protocol: ProtocolId,
    account_index: u32,
) -> Result<Vec<u8>, String> {
    let conn = db.conn.as_ref().ok_or("database is locked")?;
    let conn = conn.lock().map_err(|e| e.to_string())?;
    conn.query_row(
        "SELECT viewing_key FROM pre_derived_keys WHERE protocol = ?1 AND account_index = ?2",
        rusqlite::params![format!("{:?}", protocol), account_index],
        |row| row.get(0),
    ).map_err(|e| format!("pre-derived key not found: {e}"))
}
```

**Modify `create_account()`:**
- Remove `pre_derived_keys: &HashMap` parameter
- Add `db: &Database` parameter
- Read viewing key from `pre_derived_keys` table via `get_pre_derived_key()`
- Derive address from viewing key using `usecases::derive_address(protocols, protocol, &viewing_key, 0)`
- Set `address` in the created Account record
- Delete the pre-derived key entry after successful creation

**Add `create_ethereum_account_0()`:**
```rust
pub fn create_ethereum_account_0(
    db: &Database,
    repo: &dyn AccountsRepository,
    protocols: &ProtocolService,
) -> Result<Account, String> {
    // Check if Ethereum account 0 already exists
    let conn = db.conn.as_ref().ok_or("database is locked")?;
    let conn = conn.lock().map_err(|e| e.to_string())?;
    let existing = repo.find_by_protocol(&conn, &ProtocolId::Ethereum)?;
    drop(conn);
    
    if existing.iter().any(|a| a.derivation_path == "m/44'/60'/0'") {
        return Err("Ethereum account 0 already exists".to_string());
    }
    
    let viewing_key = get_pre_derived_key(db, ProtocolId::Ethereum, 0)?;
    let address = derive_address(protocols, ProtocolId::Ethereum, &viewing_key, 0)?;
    
    let id: String = (0..16).map(|_| format!("{:x}", rand::thread_rng().gen_range(0..16))).collect();
    let account = Account {
        id,
        protocol: ProtocolId::Ethereum,
        derivation_path: "m/44'/60'/0'".to_string(),
        name: "Ethereum Account 0".to_string(),
        address,
        viewing_key,
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    };
    
    let conn = db.conn.as_ref().ok_or("database is locked")?;
    let conn = conn.lock().map_err(|e| e.to_string())?;
    repo.save(&conn, &account)?;
    Ok(account)
}
```

### `paypunkd/src/paypunkd.rs`

**Modify `unlock()` handler:**
After keypunkd returns bulk-derived viewing keys (`derived: Vec<(ProtocolId, u32, Vec<u8>)>`):
1. For each `(protocol, account_index, viewing_key)`:
   - Call `usecases::save_pre_derived_key(&self.db, protocol, account_index, &viewing_key)`
2. Call `usecases::create_ethereum_account_0(&self.db, self.accounts_repo.as_ref(), &self.protocols)` — ignore error if already exists
3. Keep the existing `self.pre_derived_keys` HashMap update for backward compat (will be removed in Step 4)

**Modify `create_account()` handler:**
- Pass `&self.db` instead of `&self.pre_derived_keys`

### `paypunkd/src/usecases.rs` — update `create_account` signature:
```rust
pub async fn create_account(
    db: &Database,
    repo: &dyn AccountsRepository,
    protocols: &ProtocolService,
    protocol: ProtocolId,
    derivation_path: String,
    account_index: u32,
    name: String,
) -> Result<Account, String>
```

## Acceptance Criteria

- [ ] After unlock, `pre_derived_keys` table has entries for all protocols/indices
- [ ] After unlock, Ethereum Account 0 exists in `accounts` table with real derived address
- [ ] `create_account` for additional accounts reads from `pre_derived_keys` table
- [ ] `cargo build` succeeds
- [ ] `cargo test` passes

## Tests

- Integration test: generate seed, unlock, verify account 0 exists with non-empty address
- `cargo test` — existing tests still pass
