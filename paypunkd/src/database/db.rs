use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::Connection;

use super::migration::{AccountsMigration, Migrator, Migration};

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Migration error: {0}")]
    Migration(String),
}

pub struct Database {
    pub conn: Mutex<Connection>,
    #[allow(dead_code)]
    db_path: PathBuf,
    enc_path: PathBuf,
}

impl Database {
    pub fn open(data_dir: &Path) -> Result<Self, DbError> {
        std::fs::create_dir_all(data_dir)?;

        let enc_path = data_dir.join("paypunkd.db.enc");
        let db_path = data_dir.join("paypunkd.db");

        let conn = if db_path.exists() {
            Connection::open(&db_path)?
        } else {
            let conn = Connection::open(&db_path)?;
            conn
        };

        let db = Database {
            conn: Mutex::new(conn),
            db_path,
            enc_path,
        };

        db.run_migrations()?;

        Ok(db)
    }

    pub fn wallet_exists(&self) -> bool {
        self.enc_path.exists()
    }

    fn run_migrations(&self) -> Result<(), DbError> {
        let conn = self.conn.lock().map_err(|e| DbError::Migration(e.to_string()))?;
        let mut migrator = Migrator::new();
        migrator.register(Box::new(InitialMigration));
        migrator.register(Box::new(AccountsMigration));
        migrator
            .migrate(&conn)
            .map_err(DbError::Migration)?;
        Ok(())
    }

    pub fn close(self) -> Result<(), DbError> {
        {
            let conn = self.conn.lock().map_err(|e| DbError::Migration(e.to_string()))?;
            conn.execute_batch("VACUUM;")
                .map_err(DbError::Sqlite)?;
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
            .lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM _migrations", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);
        db.close().unwrap();
    }

    #[test]
    fn test_db_reopen_reads_data() {
        let dir = tempfile::TempDir::new().unwrap();
        {
            let db = Database::open(dir.path()).unwrap();
            db.conn
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
        let db = Database::open(dir.path()).unwrap();
        db.close().unwrap();
        // wallet_exists checks for .enc file, which is not created by close() anymore
        // Create it manually to test the method
        let enc_path = dir.path().join("paypunkd.db.enc");
        std::fs::write(&enc_path, b"fake encrypted data").unwrap();
        let db = Database::open(dir.path()).unwrap();
        assert!(db.wallet_exists());
        db.close().unwrap();
    }
}
