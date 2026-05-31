use paypunk_ipc::IpcMessage;
use tactix::{Actor, Ctx, Handler};

use crate::crypto::KeyStore;
use crate::key;
use crate::messages::{KeypunkdRequest, KeypunkdResponse};
use crate::seed_store::SeedStore;

/// Convenience bound for a thread-safe seed store usable inside an actor.
pub trait Storage: SeedStore + Send + Sync + 'static {}
impl<T: SeedStore + Send + Sync + 'static> Storage for T {}

pub struct Dispatcher<S: Storage> {
    keystore: KeyStore,
    seed_store: S,
}

impl<S: Storage> Dispatcher<S> {
    pub fn new(keystore: KeyStore, seed_store: S) -> Self {
        Self { keystore, seed_store }
    }
}

impl<S: Storage> Actor for Dispatcher<S> {}

impl<S: Storage> Handler<IpcMessage> for Dispatcher<S> {
    async fn handle(&mut self, msg: IpcMessage, _ctx: &Ctx<Self>) -> Result<Vec<u8>, String> {
        let request: KeypunkdRequest =
            postcard::from_bytes(&msg.0).map_err(|e| format!("deserialize error: {e}"))?;

        let response = match request {
            KeypunkdRequest::GetPublicKey => KeypunkdResponse::PublicKey {
                key: self.keystore.public_key(),
            },
            KeypunkdRequest::GenerateSeed {
                encrypted_password,
                client_public_key,
            } => match handle_generate_seed(
                &self.keystore,
                &encrypted_password,
                &client_public_key,
                &self.seed_store,
            ) {
                Ok(encrypted_mnemonic) => KeypunkdResponse::SeedGenerated { encrypted_mnemonic },
                Err(e) => KeypunkdResponse::Error {
                    message: e.to_string(),
                },
            },
        };

        postcard::to_allocvec(&response).map_err(|e| format!("serialize error: {e}"))
    }
}

fn handle_generate_seed(
    keystore: &KeyStore,
    encrypted_password: &[u8],
    client_pk: &[u8; 32],
    store: &impl SeedStore,
) -> Result<Vec<u8>, GenerateError> {
    let password = keystore.decrypt_password(encrypted_password, client_pk)?;
    let (seed, mnemonic) = key::generate_seed();
    let encrypted = key::encrypt_seed(&seed, &password)?;
    store.write(&encrypted)?;
    Ok(keystore.encrypt_mnemonic(&mnemonic, client_pk))
}

#[derive(Debug, thiserror::Error)]
enum GenerateError {
    #[error("{0}")]
    Crypto(#[from] crate::crypto::CryptoError),
    #[error("{0}")]
    Key(#[from] key::KeyError),
    #[error("{0}")]
    Store(#[from] crate::seed_store::SeedStoreError),
}
