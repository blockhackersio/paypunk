use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::Connection;

use super::encryption::{decrypt_db, encrypt_db, DbCryptoError};
use super::migration::{
    AccountsMigration, AddressBookMigration, Migration, Migrator, PreDerivedKeysMigration,
    SettingsMigration,
};

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Migration error: {0}")]
    Migration(String),
    #[error("Encryption error: {0}")]
    Crypto(#[from] DbCryptoError),
}

pub struct Database {
    pub conn: Option<Mutex<Connection>>,
    db_path: PathBuf,
    enc_path: PathBuf,
    encrypted_bytes: Option<Vec<u8>>,
    password: Option<String>,
}

impl Database {
    pub fn open(data_dir: &Path) -> Result<Self, DbError> {
        std::fs::create_dir_all(data_dir)?;

        let enc_path = data_dir.join("paypunkd.db.enc");
        let db_path = data_dir.join("paypunkd.db");

        if enc_path.exists() {
            let encrypted_bytes = std::fs::read(&enc_path)?;
            Ok(Database {
                conn: None,
                db_path,
                enc_path,
                encrypted_bytes: Some(encrypted_bytes),
                password: None,
            })
        } else {
            let conn = Connection::open(&db_path)?;
            let db = Database {
                conn: Some(Mutex::new(conn)),
                db_path,
                enc_path,
                encrypted_bytes: None,
                password: None,
            };
            db.run_migrations()?;
            Ok(db)
        }
    }

    pub fn unlock(&mut self, password: &str) -> Result<(), DbError> {
        let encrypted = self
            .encrypted_bytes
            .as_ref()
            .ok_or_else(|| DbError::Crypto(DbCryptoError::DecryptionFailed))?;

        let plaintext = decrypt_db(encrypted, password)?;

        let temp_path = self.db_path.with_extension("db.tmp");
        std::fs::write(&temp_path, &plaintext)?;
        std::fs::rename(&temp_path, &self.db_path)?;

        let conn = Connection::open(&self.db_path)?;

        self.conn = Some(Mutex::new(conn));
        self.password = Some(password.to_string());
        self.run_migrations()?;

        Ok(())
    }

    pub fn wallet_exists(&self) -> bool {
        self.enc_path.exists()
    }

    pub fn is_locked(&self) -> bool {
        self.conn.is_none()
    }

    fn run_migrations(&self) -> Result<(), DbError> {
        let conn = self
            .conn
            .as_ref()
            .ok_or_else(|| DbError::Migration("database is locked".to_string()))?;
        let conn = conn.lock().map_err(|e| DbError::Migration(e.to_string()))?;
        let mut migrator = Migrator::new();
        migrator.register(Box::new(InitialMigration));
        migrator.register(Box::new(AccountsMigration));
        migrator.register(Box::new(PreDerivedKeysMigration));
        migrator.register(Box::new(AddressBookMigration));
        migrator.register(Box::new(SettingsMigration));
        migrator.migrate(&conn).map_err(DbError::Migration)?;
        Ok(())
    }

    pub fn close(self) -> Result<(), DbError> {
        if let Some(conn) = self.conn {
            {
                let conn = conn.lock().map_err(|e| DbError::Migration(e.to_string()))?;
                conn.execute_batch("VACUUM;").map_err(DbError::Sqlite)?;
            }

            if let Some(password) = self.password {
                let plaintext = std::fs::read(&self.db_path)?;
                let encrypted = encrypt_db(&plaintext, &password)?;
                std::fs::write(&self.enc_path, &encrypted)?;
                std::fs::remove_file(&self.db_path)?;
            }
        }

        Ok(())
    }
}

struct InitialMigration;

impl Migration for InitialMigration {
    fn version(&self) -> u32 {
        1
    }

    fn up(&self, conn: &Connection) -> Result<(), String> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS accounts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );",
        )
        .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_db_create_and_migrate() {
        let dir = tempfile::TempDir::new().unwrap();
        let db = Database::open(dir.path()).unwrap();
        let count: i64 = db
            .conn
            .as_ref()
            .unwrap()
            .lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM _migrations", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 5);
        db.close().unwrap();
    }

    #[test]
    fn test_db_reopen_reads_data() {
        let dir = tempfile::TempDir::new().unwrap();
        {
            let db = Database::open(dir.path()).unwrap();
            db.conn
                .as_ref()
                .unwrap()
                .lock()
                .unwrap()
                .execute(
                    "INSERT INTO accounts (id, protocol, derivation_path, name, viewing_key, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    rusqlite::params!["test-id", "Zcash", "m/44'/133'/0'", "test", vec![1u8, 2u8, 3u8], 1000u64],
                )
                .unwrap();
            db.close().unwrap();
        }
        {
            let db = Database::open(dir.path()).unwrap();
            let count: i64 = db
                .conn
                .as_ref()
                .unwrap()
                .lock()
                .unwrap()
                .query_row("SELECT COUNT(*) FROM accounts", [], |row| row.get(0))
                .unwrap();
            assert_eq!(count, 1);
            db.close().unwrap();
        }
    }

    #[test]
    fn test_wallet_exists_false_when_no_encrypted_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let db = Database::open(dir.path()).unwrap();
        assert!(!db.wallet_exists());
        db.close().unwrap();
    }

    #[test]
    fn test_wallet_exists_true_when_encrypted_file_present() {
        let dir = tempfile::TempDir::new().unwrap();
        let enc_path = dir.path().join("paypunkd.db.enc");
        std::fs::write(&enc_path, b"fake encrypted data").unwrap();
        let db = Database::open(dir.path()).unwrap();
        assert!(db.wallet_exists());
        assert!(db.is_locked());
    }

    #[test]
    fn test_open_locked_when_encrypted_file_exists() {
        let dir = tempfile::TempDir::new().unwrap();
        let enc_path = dir.path().join("paypunkd.db.enc");
        std::fs::write(&enc_path, b"some encrypted blob").unwrap();
        let db = Database::open(dir.path()).unwrap();
        assert!(db.is_locked());
        assert!(db.conn.is_none());
    }

    #[test]
    fn test_unlock_roundtrip() {
        let dir = tempfile::TempDir::new().unwrap();
        let password = "test-password";

        // Create a real SQLite DB file to use as plaintext
        let db_path = dir.path().join("paypunkd.db");
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch(
                "CREATE TABLE test (id INTEGER PRIMARY KEY, value TEXT);
                 INSERT INTO test VALUES (1, 'hello');",
            )
            .unwrap();
        }

        // Read the plaintext and encrypt it
        let plaintext = std::fs::read(&db_path).unwrap();
        let encrypted = encrypt_db(&plaintext, password).unwrap();
        let enc_path = dir.path().join("paypunkd.db.enc");
        std::fs::write(&enc_path, &encrypted).unwrap();
        std::fs::remove_file(&db_path).unwrap();

        // Open in locked state
        let mut db = Database::open(dir.path()).unwrap();
        assert!(db.is_locked());

        // Unlock
        db.unlock(password).unwrap();
        assert!(!db.is_locked());

        // Verify data survived
        let conn = db.conn.as_ref().unwrap().lock().unwrap();
        let value: String = conn
            .query_row("SELECT value FROM test WHERE id = 1", [], |row| row.get(0))
            .unwrap();
        assert_eq!(value, "hello");
        drop(conn);

        db.close().unwrap();
        // After close, enc file should exist and plaintext removed
        assert!(enc_path.exists());
        assert!(!dir.path().join("paypunkd.db").exists());
    }

    #[test]
    fn test_unlock_wrong_password_fails() {
        let dir = tempfile::TempDir::new().unwrap();
        let password = "correct-password";

        let plaintext = b"test data";
        let encrypted = encrypt_db(plaintext, password).unwrap();
        let enc_path = dir.path().join("paypunkd.db.enc");
        std::fs::write(&enc_path, &encrypted).unwrap();

        let mut db = Database::open(dir.path()).unwrap();
        assert!(db.is_locked());

        let result = db.unlock("wrong-password");
        assert!(result.is_err());
        assert!(db.is_locked());
    }

    #[test]
    fn test_open_creates_db_when_no_encrypted_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let db = Database::open(dir.path()).unwrap();
        assert!(!db.is_locked());
        assert!(db.conn.is_some());
        db.close().unwrap();
    }
}
