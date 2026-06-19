# Goal 2: Database layer with repository pattern + encryption

## Context

`paypunkd` currently has no database — it's stateless. All state (seed) lives in `keypunkd`'s filesystem seed store. We need a wallet database in `paypunkd` to support accounts (Goal 3) and future features like transaction history.

The workspace already has `rusqlite = "0.37"` and `secrecy = "0.8"` in `Cargo.toml:65-66`.

Key design decisions (from user):
- **Database**: SQLite via `rusqlite`
- **Location**: XDG data directory (`~/.local/share/paypunk/paypunkd.db`)
- **Encryption**: Application-level AES-256-GCM. Encrypt the entire DB file with a key derived from the user's password via Argon2id with a domain-separated salt (`paypunk-db-v1`). Decrypt to a tempfile on open, re-encrypt on close.
- **The DB encryption key MUST be different from the seed encryption key** (which uses `argon2::derive_key` in `keypunkd/src/key.rs`). Use a distinct Argon2id salt like `b"paypunk-db-v1"`.
- **Repository pattern**: Abstract data access behind traits injected into usecases.

The existing `key.rs` in keypunkd shows the pattern for Argon2id + AES-256-GCM encryption:
- `keypunkd/src/key.rs:28-34`: `derive_key(password, salt)` using `Argon2::default().hash_password_into()`
- `keypunkd/src/key.rs:39-58`: `encrypt_seed()` — salt + nonce + ciphertext format
- `keypunkd/src/key.rs:63-83`: `decrypt_seed()` — parse blob, derive key, decrypt

## Implementation plan

### 1. Create `paypunkd/src/database/` module

```rust
// paypunkd/src/database/mod.rs
pub mod db;
pub mod encryption;
pub mod repository;
pub mod migration;

pub use db::Database;
pub use repository::Repository;
```

### 2. Create `paypunkd/src/database/encryption.rs`

Implement DB file encryption/decryption following the same pattern as `keypunkd/src/key.rs` but with a distinct salt:

```rust
pub struct DbEncryptionKey {
    key: [u8; 32],
}

/// Derive DB encryption key from password.
/// Uses a domain-separated salt so this key is NEVER the same as the seed encryption key.
pub fn derive_db_key(password: &str) -> DbEncryptionKey { ... }

/// Encrypt a DB file blob. Returns salt+nonce+ciphertext.
pub fn encrypt_db(plaintext: &[u8], password: &str) -> Result<Vec<u8>, DbCryptoError> { ... }

/// Decrypt a DB file blob. Expects salt+nonce+ciphertext format.
pub fn decrypt_db(blob: &[u8], password: &str) -> Result<Vec<u8>, DbCryptoError> { ... }
```

Use salt `b"paypunk-db-v1"` (domain separation from keypunkd's seed encryption which uses random salt).

### 3. Create `paypunkd/src/database/migration.rs`

Simple version-tracked schema migration:

```rust
pub trait Migration {
    fn version(&self) -> u32;
    fn up(&self, conn: &Connection) -> Result<(), String>;
}

pub struct Migrator {
    migrations: Vec<Box<dyn Migration>>,
}

impl Migrator {
    pub fn new() -> Self { ... }
    pub fn register(&mut self, migration: Box<dyn Migration>) { ... }
    /// Run all pending migrations. Tracks version in `_migrations` table.
    pub fn migrate(&self, conn: &Connection) -> Result<(), String> { ... }
}
```

The `_migrations` table schema:
```sql
CREATE TABLE IF NOT EXISTS _migrations (
    version INTEGER PRIMARY KEY,
    applied_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

### 4. Create `paypunkd/src/database/db.rs`

The `Database` struct:

```rust
pub struct Database {
    conn: Connection,
    db_path: PathBuf,
    enc_path: PathBuf,
    password: String,  // held only while open
}

impl Database {
    /// Open (or create + encrypt) the database at {data_dir}/paypunkd.db.
    /// 1. If the encrypted file exists, decrypt it to a tempfile.
    /// 2. Open the tempfile with rusqlite.
    /// 3. Run migrations.
    /// 4. Return Database handle.
    pub fn open(data_dir: &Path, password: &str) -> Result<Self, DbError> { ... }

    /// Close the database: re-encrypt the tempfile and write to the encrypted path,
    /// then zeroize the tempfile and password.
    pub fn close(self) -> Result<(), DbError> { ... }
}
```

On open:
1. Check if `{data_dir}/paypunkd.db.enc` exists
2. If yes, read it, `decrypt_db()` to get plaintext SQLite bytes, write to a tempfile
3. Open tempfile with `rusqlite::Connection::open(&tempfile_path)`
4. Run migrations
5. Return `Database { conn, db_path: tempfile, enc_path, password }`

On close:
1. Vacuum the SQLite DB to ensure it's fully written
2. Read the tempfile into memory
3. Call `encrypt_db(plaintext, &password)` 
4. Write encrypted blob to `{data_dir}/paypunkd.db.enc`
5. Delete the tempfile
6. Zeroize password

### 5. Create `paypunkd/src/database/repository.rs`

```rust
/// Base repository trait for data access.
pub trait Repository<T> {
    fn save(&self, conn: &Connection, entity: &T) -> Result<(), String>;
    fn find_all(&self, conn: &Connection) -> Result<Vec<T>, String>;
}
```

### 6. Register module in `paypunkd/src/lib.rs`

Add `pub mod database;` to `paypunkd/src/lib.rs`.

### 7. Update `paypunkd/Cargo.toml`

Add `rusqlite = { workspace = true, features = ["bundled"] }` to enable SQLite without system library dependency.

## Files to create/modify

- `paypunkd/src/database/mod.rs` — **new**: module exports
- `paypunkd/src/database/encryption.rs` — **new**: Argon2id + AES-256-GCM for DB files
- `paypunkd/src/database/migration.rs` — **new**: version-tracked migration system
- `paypunkd/src/database/db.rs` — **new**: Database struct with open/close lifecycle
- `paypunkd/src/database/repository.rs` — **new**: Repository trait
- `paypunkd/src/lib.rs` — add `pub mod database;`
- `paypunkd/Cargo.toml` — add `rusqlite` dependency

## Tests

### Unit test: `db_encryption_roundtrip`

```rust
#[test]
fn test_encrypt_decrypt_db_roundtrip() {
    let plaintext = b"CREATE TABLE test (id INTEGER);";
    let encrypted = encrypt_db(plaintext, "password").unwrap();
    let decrypted = decrypt_db(&encrypted, "password").unwrap();
    assert_eq!(decrypted, plaintext);
}
```

### Unit test: `db_encryption_wrong_password_fails`

```rust
#[test]
fn test_decrypt_db_wrong_password_fails() {
    let plaintext = b"test data";
    let encrypted = encrypt_db(plaintext, "correct-pw").unwrap();
    let result = decrypt_db(&encrypted, "wrong-pw");
    assert!(result.is_err());
}
```

### Unit test: `db_create_and_migrate`

```rust
#[test]
fn test_db_create_and_migrate() {
    let dir = tempfile::TempDir::new().unwrap();
    let db = Database::open(dir.path(), "password").unwrap();
    // Verify _migrations table exists
    let count: i64 = db.conn.query_row(
        "SELECT COUNT(*) FROM _migrations", [], |row| row.get(0)
    ).unwrap();
    assert_eq!(count, 1);  // initial migration ran
    db.close().unwrap();
}
```

### Unit test: `db_raw_file_not_readable`

```rust
#[test]
fn test_raw_encrypted_file_not_readable_sqlite() {
    let dir = tempfile::TempDir::new().unwrap();
    let db = Database::open(dir.path(), "password").unwrap();
    db.close().unwrap();

    // The .enc file should NOT be a valid SQLite file
    let enc_path = dir.path().join("paypunkd.db.enc");
    let encrypted = std::fs::read(&enc_path).unwrap();
    // SQLite header is "SQLite format 3\0" — encrypted file should not start with this
    assert_ne!(&encrypted[..16], b"SQLite format 3\0");
}
```

## Acceptance criteria

- `Database::open(config, password)` creates/opens an encrypted DB file at `{data_dir}/paypunkd.db.enc`
- Migrations run on open (schema version tracked in a `_migrations` table)
- `Repository` trait with `save()` and `find_all()` methods
- Unit test: creating a DB, writing and reading data works
- Unit test: opening with wrong password fails
- Unit test: the raw DB file is not readable SQLite without decryption
