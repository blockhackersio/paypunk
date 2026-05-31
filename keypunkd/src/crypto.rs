use aes_gcm::aead::{Aead, OsRng};
use aes_gcm::{Aes256Gcm, Key, KeyInit, Nonce};
use blake2::digest::consts::U32;
use blake2::Digest;
use rand::RngCore;

#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("decryption failed: {0}")]
    Decryption(String),
    #[error("invalid encrypted blob")]
    InvalidBlob,
}

const NONCE_LEN: usize = 12;

// ---------------------------------------------------------------------------
// Shared: derive AES-256 key from an X25519 shared secret via Blake2b
// ---------------------------------------------------------------------------

fn derive_aes_key(shared_secret: &[u8; 32]) -> Key<Aes256Gcm> {
    let mut hasher = blake2::Blake2b::<U32>::new();
    hasher.update(shared_secret.as_slice());
    let result = hasher.finalize();
    *Key::<Aes256Gcm>::from_slice(&result)
}

// ---------------------------------------------------------------------------
// Shared: encrypt/decrypt helpers
// ---------------------------------------------------------------------------

fn encrypt(key: &Key<Aes256Gcm>, plaintext: &[u8]) -> Vec<u8> {
    let mut nonce_bytes = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let cipher = Aes256Gcm::new(key);
    let ciphertext = cipher.encrypt(nonce, plaintext).expect("AES-GCM encrypt");
    let mut blob = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    blob.extend_from_slice(&nonce_bytes);
    blob.extend_from_slice(&ciphertext);
    blob
}

fn decrypt(key: &Key<Aes256Gcm>, blob: &[u8]) -> Result<Vec<u8>, CryptoError> {
    if blob.len() < NONCE_LEN {
        return Err(CryptoError::InvalidBlob);
    }
    let (nonce_bytes, ciphertext) = blob.split_at(NONCE_LEN);
    let nonce = Nonce::from_slice(nonce_bytes);
    let cipher = Aes256Gcm::new(key);
    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| CryptoError::Decryption(e.to_string()))
}

/// Generate a random X25519 keypair. Returns (secret_scalar, public_key).
fn generate_keypair() -> ([u8; 32], [u8; 32]) {
    let mut secret = [0u8; 32];
    OsRng.fill_bytes(&mut secret);
    // Clamp per RFC 7748
    secret[0] &= 248;
    secret[31] &= 127;
    secret[31] |= 64;
    let public = x25519_dalek::x25519(secret, x25519_dalek::X25519_BASEPOINT_BYTES);
    (secret, public)
}

// ---------------------------------------------------------------------------
// Server side — long-lived keypair held by keypunkd
// ---------------------------------------------------------------------------

pub struct KeyStore {
    secret: [u8; 32],
    public: [u8; 32],
}

impl KeyStore {
    pub fn new() -> Self {
        let (secret, public) = generate_keypair();
        Self { secret, public }
    }

    pub fn public_key(&self) -> [u8; 32] {
        self.public
    }

    pub fn decrypt_password(
        &self,
        encrypted: &[u8],
        client_pk: &[u8; 32],
    ) -> Result<String, CryptoError> {
        let shared = x25519_dalek::x25519(self.secret, *client_pk);
        let key = derive_aes_key(&shared);
        let plaintext = decrypt(&key, encrypted)?;
        String::from_utf8(plaintext).map_err(|_| CryptoError::Decryption("invalid utf-8".into()))
    }

    pub fn encrypt_mnemonic(&self, mnemonic: &str, client_pk: &[u8; 32]) -> Vec<u8> {
        let shared = x25519_dalek::x25519(self.secret, *client_pk);
        let key = derive_aes_key(&shared);
        encrypt(&key, mnemonic.as_bytes())
    }
}

// ---------------------------------------------------------------------------
// Client side — ephemeral keypair for a single GenerateSeed call
// ---------------------------------------------------------------------------

pub struct CryptoSession {
    secret: [u8; 32],
    public: [u8; 32],
}

impl CryptoSession {
    pub fn new() -> Self {
        let (secret, public) = generate_keypair();
        Self { secret, public }
    }

    pub fn public_key(&self) -> [u8; 32] {
        self.public
    }

    pub fn seal_password(&self, password: &str, server_pk: &[u8; 32]) -> Vec<u8> {
        let shared = x25519_dalek::x25519(self.secret, *server_pk);
        let key = derive_aes_key(&shared);
        encrypt(&key, password.as_bytes())
    }

    pub fn open_mnemonic(
        &self,
        encrypted: &[u8],
        server_pk: &[u8; 32],
    ) -> Result<String, CryptoError> {
        let shared = x25519_dalek::x25519(self.secret, *server_pk);
        let key = derive_aes_key(&shared);
        let plaintext = decrypt(&key, encrypted)?;
        String::from_utf8(plaintext).map_err(|_| CryptoError::Decryption("invalid utf-8".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_password_roundtrip() {
        let server = KeyStore::new();
        let client = CryptoSession::new();

        let password = "my-secret-password";
        let encrypted = client.seal_password(password, &server.public_key());
        let decrypted = server.decrypt_password(&encrypted, &client.public_key()).unwrap();

        assert_eq!(decrypted, password);
    }

    #[test]
    fn test_mnemonic_roundtrip() {
        let server = KeyStore::new();
        let client = CryptoSession::new();

        let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let encrypted = server.encrypt_mnemonic(mnemonic, &client.public_key());
        let decrypted = client.open_mnemonic(&encrypted, &server.public_key()).unwrap();

        assert_eq!(decrypted, mnemonic);
    }

    #[test]
    fn test_wrong_key_fails() {
        let server = KeyStore::new();
        let other_server = KeyStore::new();
        let client = CryptoSession::new();

        let password = "secret";
        let encrypted = client.seal_password(password, &server.public_key());

        let result = other_server.decrypt_password(&encrypted, &client.public_key());
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_blob_fails() {
        let server = KeyStore::new();
        let result = server.decrypt_password(&[1, 2, 3], &[0u8; 32]);
        assert!(result.is_err());
    }

    #[test]
    fn test_server_reuses_key() {
        let server = KeyStore::new();
        let client1 = CryptoSession::new();
        let client2 = CryptoSession::new();

        // Server should handle multiple clients with the same key
        let enc1 = client1.seal_password("pw1", &server.public_key());
        let enc2 = client2.seal_password("pw2", &server.public_key());

        assert_eq!(server.decrypt_password(&enc1, &client1.public_key()).unwrap(), "pw1");
        assert_eq!(server.decrypt_password(&enc2, &client2.public_key()).unwrap(), "pw2");
    }
}
