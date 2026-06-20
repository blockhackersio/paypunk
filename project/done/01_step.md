# Pre-hash password with Argon2id before sending to daemons

## Context

The raw password was being encrypted with X25519 and sent to both paypunkd (for DB unlock) and keypunkd (for seed decryption). The architecture requires that paypunkd never sees the raw password. Instead, the API layer pre-hashes the password with Argon2id using domain-separation salts before encrypting and sending.

## Changes

### `api/Cargo.toml`
Add `argon2.workspace = true` and `hex.workspace = true` dependencies.

### `api/src/functions.rs`

**New imports:**
```rust
use argon2::Argon2;
```

**New helper function** (before `check_wallet_exists`):
```rust
/// Hash a password with a domain-separation salt using Argon2id.
///
/// Returns a 64-character hex-encoded string of the 32-byte hash.
/// The domain salt ensures the same password produces different hashes
/// for different domains (e.g., paypunkd vs keypunkd).
fn hash_for_domain(password: &str, domain: &[u8]) -> Zeroizing<String> {
    let mut hash = [0u8; 32];
    Argon2::default()
        .hash_password_into(password.as_bytes(), domain, &mut hash)
        .expect("Argon2id key derivation should not fail with valid parameters");
    Zeroizing::new(hex::encode(hash))
}
```

**Modify `unlock()`** — lines 30-31:
```rust
    // Before:
    let encrypted_keypunkd_password = client_keypair.encrypt(password.clone(), &keypunk_pk);
    let encrypted_db_password = client_keypair.encrypt(password, &paypunkd_pk);

    // After:
    let encrypted_keypunkd_password =
        client_keypair.encrypt(hash_for_domain(&password, b"keypunkd-seed-key"), &keypunk_pk);
    let encrypted_db_password =
        client_keypair.encrypt(hash_for_domain(&password, b"paypunkd-db-key"), &paypunkd_pk);
```

**Modify `generate_seed()`** — line 50:
```rust
    // Before:
    let encrypted_password = client_keypair.encrypt(password, &server_pk);
    // After:
    let encrypted_password =
        client_keypair.encrypt(hash_for_domain(&password, b"keypunkd-seed-key"), &server_pk);
```

**Modify `restore_seed()`** — line 70:
```rust
    // Before:
    let encrypted_password = client_keypair.encrypt(password, &server_pk);
    // After:
    let encrypted_password =
        client_keypair.encrypt(hash_for_domain(&password, b"keypunkd-seed-key"), &server_pk);
```

**Modify `derive_address()`** — line 88:
```rust
    // Before:
    let encrypted_password = client_keypair.encrypt(password, &server_pk);
    // After:
    let encrypted_password =
        client_keypair.encrypt(hash_for_domain(&password, b"keypunkd-seed-key"), &server_pk);
```

**Modify `approve_signature()`** — lines 138-144:
```rust
    // Before:
    // Encode payload: raw_len(4) + raw + sig_len(4) + sig + pw
    let mut payload = Vec::new();
    payload.extend_from_slice(&(raw_artifact.len() as u32).to_le_bytes());
    payload.extend_from_slice(raw_artifact);
    payload.extend_from_slice(&(keypunkd_signature.len() as u32).to_le_bytes());
    payload.extend_from_slice(keypunkd_signature);
    payload.extend_from_slice(password.as_bytes());

    // After:
    let hashed_password = hash_for_domain(&password, b"keypunkd-seed-key");
    // Encode payload: raw_len(4) + raw + sig_len(4) + sig + hashed_pw
    let mut payload = Vec::new();
    payload.extend_from_slice(&(raw_artifact.len() as u32).to_le_bytes());
    payload.extend_from_slice(raw_artifact);
    payload.extend_from_slice(&(keypunkd_signature.len() as u32).to_le_bytes());
    payload.extend_from_slice(keypunkd_signature);
    payload.extend_from_slice(hashed_password.as_bytes());
```

**New tests** at bottom of file:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_for_domain_returns_hex_string() {
        let hash = hash_for_domain("mypassword", b"test-domain");
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_hash_for_domain_deterministic() {
        let a = hash_for_domain("password", b"domain");
        let b = hash_for_domain("password", b"domain");
        assert_eq!(*a, *b);
    }

    #[test]
    fn test_hash_for_domain_different_domains() {
        let a = hash_for_domain("password", b"domain-a");
        let b = hash_for_domain("password", b"domain-b");
        assert_ne!(*a, *b);
    }

    #[test]
    fn test_hash_for_domain_different_passwords() {
        let a = hash_for_domain("password-one", b"domain");
        let b = hash_for_domain("password-two", b"domain");
        assert_ne!(*a, *b);
    }
}
```

## Acceptance criteria

- `cargo build` (full workspace) succeeds with no warnings
- `cargo test` (all crates) passes
- paypunkd and keypunkd receive hex-encoded Argon2id hashes instead of raw passwords
- No changes to any crate other than `paypunk-api`
