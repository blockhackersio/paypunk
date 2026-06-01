use zeroize::Zeroizing;
use tracing::debug;

use crate::{crypto::Keypair, errors::GenerateError, key, seed_store::SeedStore};

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
