use k256::elliptic_curve::sec1::ToEncodedPoint;
use sha3::{Digest, Keccak256};
use std::str::FromStr;

#[derive(Debug, thiserror::Error)]
pub enum DeriveError {
    #[error("BIP32 derivation failed: {0}")]
    Bip32(#[from] bip32::Error),
    #[error("Invalid account: {0}")]
    InvalidAccount(u32),
    #[error("Invalid path: {0}")]
    InvalidPath(String),
}

/// Derive an Ethereum address from a BIP39 seed at the given account and
/// address index (BIP44: m/44'/60'/{account}'/0/{index}).
pub fn derive_address(seed: &[u8; 64], account: u32, index: u32) -> Result<String, DeriveError> {
    let path = format!("m/44'/60'/{account}'/0/{index}");
    let parsed = bip32::DerivationPath::from_str(&path)
        .map_err(|e| DeriveError::InvalidPath(e.to_string()))?;
    let key = bip32::ExtendedPrivateKey::<k256::ecdsa::SigningKey>::derive_from_path(*seed, &parsed)?;
    let ext_pubkey = key.public_key();
    let inner = ext_pubkey.public_key();
    let point = inner.to_encoded_point(false);
    let hash = Keccak256::digest(&point.as_bytes()[1..]);
    let address_bytes = &hash[12..];
    Ok(format!("0x{}", hex::encode(address_bytes)))
}

/// Derive an Ethereum address using the default account (0) and the given
/// address index.
pub fn derive_address_at_index(seed: &[u8; 64], index: u32) -> Result<String, DeriveError> {
    derive_address(seed, 0, index)
}

/// Derive an Ethereum address from serialized public key bytes.
/// Accepts uncompressed (65 bytes, starts with 0x04) or compressed (33 bytes)
/// SEC1-encoded public keys.
pub fn derive_from_pubkey(pubkey_bytes: &[u8]) -> Result<String, DeriveError> {
    let point = k256::PublicKey::from_sec1_bytes(pubkey_bytes)
        .map_err(|_| DeriveError::InvalidPath("invalid public key bytes".to_string()))?;
    let encoded = point.to_encoded_point(false);
    let hash = Keccak256::digest(&encoded.as_bytes()[1..]);
    let address_bytes = &hash[12..];
    Ok(format!("0x{}", hex::encode(address_bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bip39::Mnemonic;

    const MNEMONIC: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    fn seed_from_mnemonic() -> [u8; 64] {
        let mnemonic: Mnemonic = MNEMONIC.parse().unwrap();
        mnemonic.to_seed("")
    }

    #[test]
    fn test_derive_ethereum_address() {
        let seed = seed_from_mnemonic();
        let address = derive_address(&seed, 0, 0).unwrap();
        // Known Ethereum address for m/44'/60'/0'/0/0 with the standard test mnemonic
        assert_eq!(
            address,
            "0x9858effd232b4033e47d90003d41ec34ecaeda94",
            "got {address}"
        );
    }

    #[test]
    fn test_derive_different_indexes() {
        let seed = seed_from_mnemonic();
        let a0 = derive_address(&seed, 0, 0).unwrap();
        let a1 = derive_address(&seed, 0, 1).unwrap();
        assert_ne!(a0, a1, "addresses at different indexes must differ");
    }

    #[test]
    fn test_derive_is_deterministic() {
        let seed = seed_from_mnemonic();
        let a = derive_address(&seed, 0, 0).unwrap();
        let b = derive_address(&seed, 0, 0).unwrap();
        assert_eq!(a, b, "derivation must be deterministic");
    }

    #[test]
    fn test_derive_from_pubkey_roundtrip() {
        let seed = seed_from_mnemonic();
        let address = derive_address(&seed, 0, 0).unwrap();

        // Derive the public key the same way
        let path = bip32::DerivationPath::from_str("m/44'/60'/0'/0/0").unwrap();
        let key = bip32::ExtendedPrivateKey::<k256::ecdsa::SigningKey>::derive_from_path(
            seed,
            &path,
        )
        .unwrap();
        let ext_pubkey = key.public_key();
        let inner = ext_pubkey.public_key();
        let point = inner.to_encoded_point(false);
        let recovered = derive_from_pubkey(point.as_bytes()).unwrap();
        assert_eq!(address, recovered);
    }
}
