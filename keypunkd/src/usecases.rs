use bip39::Mnemonic;
use zeroize::Zeroizing;
use tracing::debug;

use crate::{
    crypto::Keypair,
    errors::{GenerateError, RestoreError},
    key,
    seed_store::SeedStore,
};

pub fn generate_seed(
    keystore: &Keypair,
    encrypted_password: &[u8],
    client_pk: &[u8; 32],
    store: &impl SeedStore,
) -> Result<Vec<u8>, GenerateError> {
    debug!("decrypting password");
    let password = keystore.decrypt(encrypted_password, client_pk)?;

    debug!("generating BIP39 seed");
    let (seed, mnemonic) = key::generate_seed();

    debug!("encrypting seed with password");
    let encrypted = key::encrypt_seed(&seed, &*password)?;

    debug!("persisting encrypted seed");
    store.write(&encrypted)?;

    let mnemonic = Zeroizing::new(mnemonic);
    debug!("encrypting mnemonic for client");
    Ok(keystore.encrypt(mnemonic, client_pk))
}

pub fn restore_seed(
    keystore: &Keypair,
    encrypted_mnemonic: &[u8],
    encrypted_password: &[u8],
    client_pk: &[u8; 32],
    store: &impl SeedStore,
) -> Result<(), RestoreError> {
    debug!("decrypting password");
    let password = keystore.decrypt(encrypted_password, client_pk)?;

    debug!("decrypting mnemonic");
    let mnemonic_str = keystore.decrypt(encrypted_mnemonic, client_pk)?;

    debug!("validating BIP39 mnemonic");
    let mnemonic = Mnemonic::parse_in(bip39::Language::English, &*mnemonic_str)
        .map_err(|e| RestoreError::InvalidMnemonic(e.to_string()))?;

    let seed_bytes = mnemonic.to_seed_normalized("");
    let mut seed = [0u8; 64];
    seed.copy_from_slice(&seed_bytes);

    debug!("encrypting seed with password");
    let encrypted = key::encrypt_seed(&seed, &*password)?;

    debug!("persisting encrypted seed");
    store.write(&encrypted)?;

    Ok(())
}
