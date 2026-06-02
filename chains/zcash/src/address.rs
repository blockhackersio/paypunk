use orchard::keys::{FullViewingKey, Scope, SpendingKey};
use zcash_address::unified::{self, Encoding};
use zcash_address::{Network, ToAddress, ZcashAddress};

#[derive(Debug, thiserror::Error)]
pub enum DeriveError {
    #[error("ZIP 32 derivation failed: {0}")]
    Zip32(orchard::zip32::Error),
    #[error("Invalid account: {0}")]
    InvalidAccount(u32),
    #[error("Unified address encoding failed")]
    Encoding,
}

/// Derive a unified Zcash address from a BIP39 seed at the given account and
/// diversifier index.
///
/// Uses Orchard (preferred pool) for the unified address.
pub fn derive_address(seed: &[u8; 64], account: u32, index: u32) -> Result<String, DeriveError> {
    let account_id =
        zip32::AccountId::try_from(account).map_err(|_| DeriveError::InvalidAccount(account))?;
    let sk =
        SpendingKey::from_zip32_seed(seed, 133, account_id).map_err(DeriveError::Zip32)?;
    let fvk = FullViewingKey::from(&sk);
    let address = fvk.address_at(index, Scope::External);
    let raw = address.to_raw_address_bytes();

    let ua = unified::Address::try_from_items(vec![unified::Receiver::Orchard(raw)])
        .map_err(|_| DeriveError::Encoding)?;
    let zaddr = ZcashAddress::from_unified(Network::Main, ua);
    Ok(zaddr.encode())
}

/// Derive a unified address using the default account (0) and the given
/// diversifier index.
pub fn derive_address_at_index(seed: &[u8; 64], index: u32) -> Result<String, DeriveError> {
    derive_address(seed, 0, index)
}

#[cfg(test)]
pub mod tests {
    use super::*;

    fn test_seed() -> [u8; 64] {
        let mut seed = [0u8; 64];
        let mnemonic = bip39::Mnemonic::parse_in(
            bip39::Language::English,
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
        )
        .expect("valid mnemonic");
        let seed_bytes = mnemonic.to_seed_normalized("");
        seed.copy_from_slice(&seed_bytes);
        seed
    }

    #[test]
    fn test_derive_orchard_address() {
        let seed = test_seed();
        let addr = derive_address_at_index(&seed, 0).expect("should derive address");
        assert!(addr.starts_with("u1"), "got: {addr}");
        assert!(addr.len() > 50, "got: {addr}");
    }

    #[test]
    fn test_derive_different_indexes() {
        let seed = test_seed();
        let addr0 = derive_address_at_index(&seed, 0).expect("index 0");
        let addr1 = derive_address_at_index(&seed, 1).expect("index 1");
        assert_ne!(
            addr0, addr1,
            "different indexes should give different addresses"
        );
    }

    #[test]
    fn test_derive_is_deterministic() {
        let seed = test_seed();
        let a = derive_address_at_index(&seed, 0).expect("first");
        let b = derive_address_at_index(&seed, 0).expect("second");
        assert_eq!(a, b, "same seed + index should give same address");
    }
}
