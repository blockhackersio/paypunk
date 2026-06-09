use bip39::Mnemonic;
use paypunk_types::ProtocolId;
use tracing::debug;
use zeroize::Zeroizing;

use crate::{
    crypto::Keypair,
    errors::{GenerateError, RestoreError},
    key,
    protocol::ProtocolRegistry,
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

/// Decrypt the seed from the store using the given password.
///
/// Returns the 64-byte BIP39 seed.
pub fn decrypt_seed(
    encrypted_password: &[u8],
    client_pk: &[u8; 32],
    keystore: &Keypair,
    store: &impl SeedStore,
) -> Result<[u8; 64], String> {
    debug!("decrypting password");
    let password = keystore
        .decrypt(encrypted_password, client_pk)
        .map_err(|e| format!("password decryption failed: {e}"))?;

    debug!("reading encrypted seed from store");
    let encrypted = store
        .read()
        .map_err(|e| format!("read seed failed: {e}"))?
        .ok_or_else(|| "no seed found — wallet not initialized".to_string())?;

    debug!("decrypting seed");
    key::decrypt_seed(&encrypted, &*password)
        .map_err(|e| format!("seed decryption failed: {e}"))
}

/// Derive public key material for the given protocol and account.
pub fn derive_public_key(
    seed: &[u8; 64],
    registry: &ProtocolRegistry,
    protocol: ProtocolId,
    account: u32,
) -> Result<Vec<u8>, String> {
    let deriver = registry
        .get(protocol)
        .ok_or_else(|| format!("unknown protocol: {protocol:?}"))?;
    deriver.derive_public_key(seed, account)
}

/// Sign a transaction with the derived private key for the given protocol and account.
pub fn sign(
    seed: &[u8; 64],
    registry: &ProtocolRegistry,
    protocol: ProtocolId,
    account: u32,
    payload: &[u8],
) -> Result<Vec<u8>, String> {
    let deriver = registry
        .get(protocol)
        .ok_or_else(|| format!("unknown protocol: {protocol:?}"))?;
    deriver.sign_transaction(seed, account, payload)
}
