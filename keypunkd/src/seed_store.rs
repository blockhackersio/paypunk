use std::path::Path;
use std::sync::Mutex;

/// How the encrypted seed blob is persisted.
pub trait SeedStore {
    fn write(&self, blob: &[u8]) -> Result<(), SeedStoreError>;
    fn read(&self) -> Result<Option<Vec<u8>>, SeedStoreError>;
}

#[derive(Debug, thiserror::Error)]
pub enum SeedStoreError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Persists the encrypted seed to `seed.enc` on the filesystem.
pub struct FilesystemSeedStore {
    path: Box<Path>,
}

impl FilesystemSeedStore {
    pub fn new(path: impl Into<Box<Path>>) -> Self {
        Self { path: path.into() }
    }
}

impl SeedStore for FilesystemSeedStore {
    fn write(&self, blob: &[u8]) -> Result<(), SeedStoreError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp_path = self.path.with_extension(".enc.tmp");
        std::fs::write(&tmp_path, blob)?;
        std::fs::rename(&tmp_path, &self.path)?;
        Ok(())
    }

    fn read(&self) -> Result<Option<Vec<u8>>, SeedStoreError> {
        if !self.path.exists() {
            return Ok(None);
        }
        Ok(Some(std::fs::read(&self.path)?))
    }
}

/// Holds the encrypted seed in memory — no filesystem access.
pub struct InMemorySeedStore {
    blob: Mutex<Option<Vec<u8>>>,
}

impl InMemorySeedStore {
    pub fn new() -> Self {
        Self {
            blob: Mutex::new(None),
        }
    }
}

impl SeedStore for InMemorySeedStore {
    fn write(&self, blob: &[u8]) -> Result<(), SeedStoreError> {
        let mut guard = self.blob.lock().expect("lock poisoned");
        *guard = Some(blob.to_vec());
        Ok(())
    }

    fn read(&self) -> Result<Option<Vec<u8>>, SeedStoreError> {
        let guard = self.blob.lock().expect("lock poisoned");
        Ok(guard.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_filesystem_store_writes_atomically() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("seed.enc");
        let store = FilesystemSeedStore::new(path.clone().into_boxed_path());
        let blob = vec![1, 2, 3, 4];

        store.write(&blob).unwrap();
        assert!(path.exists());
        assert_eq!(std::fs::read(&path).unwrap(), blob);
        assert!(!path.with_extension(".enc.tmp").exists());
    }

    #[test]
    fn test_in_memory_store_holds_blob() {
        let store = InMemorySeedStore::new();
        let blob = vec![5, 6, 7, 8];
        store.write(&blob).unwrap();
        let guard = store.blob.lock().unwrap();
        assert_eq!(guard.as_ref(), Some(&blob));
    }
}
