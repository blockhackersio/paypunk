use aes_gcm::aead::{Aead, OsRng};
use aes_gcm::{AeadCore, Aes256Gcm, Key, KeyInit, Nonce};
use argon2::Argon2;
use bip39::{Language, Mnemonic};
use rand::RngCore;

#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    #[error("Encryption error: {0}")]
    Crypto(String),
}

const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const KEY_LEN: usize = 32;

/// Generate a 12-word BIP39 mnemonic and derive the 512-bit seed.
pub fn generate_seed() -> ([u8; 64], String) {
    let mnemonic = Mnemonic::generate_in(Language::English, 12)
        .expect("12-word mnemonic generation is infallible");
    let seed = mnemonic.to_seed_normalized("");
    let mut bytes = [0u8; 64];
    bytes.copy_from_slice(&seed);
    (bytes, mnemonic.to_string())
}

/// Derive a 256-bit encryption key from password using Argon2id.
fn derive_key(password: &str, salt: &[u8]) -> [u8; KEY_LEN] {
    let mut key = [0u8; KEY_LEN];
    Argon2::default()
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .expect("Argon2id key derivation should not fail with valid parameters");
    key
}

/// Encrypt a 64-byte seed with a password using Argon2id + AES-256-GCM.
///
/// Returns a blob: [salt (16 bytes)] [nonce (12 bytes)] [ciphertext].
pub fn encrypt_seed(seed: &[u8; 64], password: &str) -> Result<Vec<u8>, KeyError> {
    let mut salt = [0u8; SALT_LEN];
    OsRng.fill_bytes(&mut salt);

    let derived_key = derive_key(password, &salt);
    let key = Key::<Aes256Gcm>::from_slice(&derived_key);
    let cipher = Aes256Gcm::new(key);
    let nonce_bytes = Aes256Gcm::generate_nonce(&mut OsRng);
    let nonce = Nonce::from_slice(nonce_bytes.as_slice());

    let ciphertext = cipher
        .encrypt(nonce, seed.as_ref())
        .map_err(|e| KeyError::Crypto(e.to_string()))?;

    let mut blob = Vec::with_capacity(SALT_LEN + NONCE_LEN + ciphertext.len());
    blob.extend_from_slice(&salt);
    blob.extend_from_slice(nonce.as_slice());
    blob.extend_from_slice(&ciphertext);
    Ok(blob)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_seed_returns_64_bytes_and_mnemonic() {
        let (seed, mnemonic) = generate_seed();
        assert_eq!(seed.len(), 64);
        assert!(!mnemonic.is_empty());
        assert_eq!(mnemonic.split_whitespace().count(), 12);
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let (seed, _) = generate_seed();
        let password = "test-password-123";

        let encrypted = encrypt_seed(&seed, password).unwrap();

        // Blob should be salt + nonce + ciphertext
        assert!(encrypted.len() > SALT_LEN + NONCE_LEN);

        // Verify we can decrypt it
        let salt = &encrypted[..SALT_LEN];
        let nonce = &encrypted[SALT_LEN..SALT_LEN + NONCE_LEN];
        let ciphertext = &encrypted[SALT_LEN + NONCE_LEN..];

        let derived_key = derive_key(password, salt);
        let key = Key::<Aes256Gcm>::from_slice(&derived_key);
        let cipher = Aes256Gcm::new(key);
        let nonce = Nonce::from_slice(nonce);

        let decrypted = cipher
            .decrypt(nonce, ciphertext)
            .expect("should decrypt successfully");

        assert_eq!(decrypted.as_slice(), &seed[..]);
    }
}
