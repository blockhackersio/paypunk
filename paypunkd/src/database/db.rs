use std::path::{Path, PathBuf};

use rusqlite::Connection;

use super::encryption::{decrypt_db, encrypt_db};
use super::migration::{Migrator, Migration};

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Crypto error: {0}")]
    Crypto(#[from] super::encryption::DbCryptoError),
    #[error("Migration error: {0}")]
    Migration(String),
}

pub struct Database {
    pub conn: Connection,
    enc_path: PathBuf,
    temp_path: PathBuf,
    password: String,
}

impl Database {
    pub fn open(data_dir: &Path, password: &str) -> Result<Self, DbError> {
        std::fs::create_dir_all(data_dir)?;

        let enc_path = data_dir.join("paypunkd.db.enc");

        let temp_path = if enc_path.exists() {
            let encrypted = std::fs::read(&enc_path)?;
            let plaintext = decrypt_db(&encrypted, password)?;
            let tmp = tempfile::Builder::new()
                .prefix("paypunkd")
                .suffix(".db")
                .tempfile()
                .map_err(|e| DbError::Io(e))?;
            std::fs::write(tmp.path(), &plaintext)?;
            let (_, path) = tmp
                .keep()
                .map_err(|e| DbError::Io(e.error))?;
            path
        } else {
            let tmp = tempfile::Builder::new()
                .prefix("paypunkd")
                .suffix(".db")
                .tempfile()
                .map_err(|e| DbError::Io(e))?;
            let (_, path) = tmp
                .keep()
                .map_err(|e| DbError::Io(e.error))?;
            path
        };

        let conn = Connection::open(&temp_path)?;

        let mut db = Database {
            conn,
            enc_path,
            temp_path,
            password: password.to_string(),
        };

        db.run_migrations()?;

        Ok(db)
    }

    fn run_migrations(&mut self) -> Result<(), DbError> {
        let mut migrator = Migrator::new();
        migrator.register(Box::new(InitialMigration));
        migrator
            .migrate(&self.conn)
            .map_err(DbError::Migration)?;
        Ok(())
    }

    pub fn close(self) -> Result<(), DbError> {
        self.conn
            .execute_batch("VACUUM;")
            .map_err(DbError::Sqlite)?;

        let plaintext = std::fs::read(&self.temp_path)?;
        let encrypted = encrypt_db(&plaintext, &self.password)?;
        std::fs::write(&self.enc_path, &encrypted)?;

        std::fs::remove_file(&self.temp_path)?;

        drop(self.password);

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
        let db = Database::open(dir.path(), "password").unwrap();
        let count: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM _migrations", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
        db.close().unwrap();
    }

    #[test]
    fn test_db_reopen_reads_data() {
        let dir = tempfile::TempDir::new().unwrap();
        {
            let db = Database::open(dir.path(), "password").unwrap();
            db.conn
                .execute("INSERT INTO accounts (name) VALUES (?1)", ["test"])
                .unwrap();
            db.close().unwrap();
        }
        {
            let db = Database::open(dir.path(), "password").unwrap();
            let count: i64 = db
                .conn
                .query_row("SELECT COUNT(*) FROM accounts", [], |row| row.get(0))
                .unwrap();
            assert_eq!(count, 1);
            db.close().unwrap();
        }
    }

    #[test]
    fn test_raw_encrypted_file_not_readable_sqlite() {
        let dir = tempfile::TempDir::new().unwrap();
        let db = Database::open(dir.path(), "password").unwrap();
        db.close().unwrap();

        let enc_path = dir.path().join("paypunkd.db.enc");
        let encrypted = std::fs::read(&enc_path).unwrap();
        assert_ne!(&encrypted[..16], b"SQLite format 3\0");
    }

    #[test]
    fn test_open_with_wrong_password_fails() {
        let dir = tempfile::TempDir::new().unwrap();
        {
            let db = Database::open(dir.path(), "correct-password").unwrap();
            db.close().unwrap();
        }
        let result = Database::open(dir.path(), "wrong-password");
        assert!(result.is_err());
    }
}
