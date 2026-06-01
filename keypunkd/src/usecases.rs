use zeroize::Zeroizing;

use crate::{crypto::Keypair, errors::GenerateError, key, seed_store::SeedStore};

pub fn generate_seed(
    keystore: &Keypair,
    encrypted_password: &[u8],
    client_pk: &[u8; 32],
    store: &impl SeedStore,
) -> Result<Vec<u8>, GenerateError> {
    let password = keystore.decrypt(encrypted_password, client_pk)?;
    let (seed, mnemonic) = key::generate_seed();
    let encrypted = key::encrypt_seed(&seed, &*password)?;
    store.write(&encrypted)?;
    let mnemonic = Zeroizing::new(mnemonic);
    Ok(keystore.encrypt(mnemonic, client_pk))
}
