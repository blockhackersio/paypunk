use aes_gcm::aead::{Aead, OsRng};
use aes_gcm::{AeadCore, Aes256Gcm, Key, KeyInit, Nonce};
use argon2::Argon2;
use rand::RngCore;

#[derive(Debug, thiserror::Error)]
pub enum DbCryptoError {
    #[error("Encryption error: {0}")]
    Crypto(String),
    #[error("Decryption failed: wrong password or corrupted data")]
    DecryptionFailed,
}

const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const KEY_LEN: usize = 32;
const DB_SALT: &[u8] = b"paypunk-db-v1";

fn derive_db_key(password: &str) -> [u8; KEY_LEN] {
    let mut key = [0u8; KEY_LEN];
    Argon2::default()
        .hash_password_into(password.as_bytes(), DB_SALT, &mut key)
        .expect("Argon2id key derivation should not fail with valid parameters");
    key
}

pub fn encrypt_db(plaintext: &[u8], password: &str) -> Result<Vec<u8>, DbCryptoError> {
    let mut salt = [0u8; SALT_LEN];
    OsRng.fill_bytes(&mut salt);

    let derived_key = derive_db_key(password);
    let key = Key::<Aes256Gcm>::from_slice(&derived_key);
    let cipher = Aes256Gcm::new(key);
    let nonce_bytes = Aes256Gcm::generate_nonce(&mut OsRng);
    let nonce = Nonce::from_slice(nonce_bytes.as_slice());

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e: aes_gcm::aead::Error| DbCryptoError::Crypto(e.to_string()))?;

    let mut blob = Vec::with_capacity(SALT_LEN + NONCE_LEN + ciphertext.len());
    blob.extend_from_slice(&salt);
    blob.extend_from_slice(nonce.as_slice());
    blob.extend_from_slice(&ciphertext);
    Ok(blob)
}

pub fn decrypt_db(blob: &[u8], password: &str) -> Result<Vec<u8>, DbCryptoError> {
    if blob.len() < SALT_LEN + NONCE_LEN {
        return Err(DbCryptoError::DecryptionFailed);
    }
    let _salt = &blob[..SALT_LEN];
    let nonce = &blob[SALT_LEN..SALT_LEN + NONCE_LEN];
    let ciphertext = &blob[SALT_LEN + NONCE_LEN..];

    let derived_key = derive_db_key(password);
    let key = Key::<Aes256Gcm>::from_slice(&derived_key);
    let cipher = Aes256Gcm::new(key);
    let nonce = Nonce::from_slice(nonce);

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| DbCryptoError::DecryptionFailed)?;

    Ok(plaintext)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_db_roundtrip() {
        let plaintext = b"CREATE TABLE test (id INTEGER);";
        let encrypted = encrypt_db(plaintext, "password").unwrap();
        let decrypted = decrypt_db(&encrypted, "password").unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_decrypt_db_wrong_password_fails() {
        let plaintext = b"test data";
        let encrypted = encrypt_db(plaintext, "correct-pw").unwrap();
        let result = decrypt_db(&encrypted, "wrong-pw");
        assert!(result.is_err());
    }
}
