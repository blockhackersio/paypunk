use paypunk_ipc::IpcMessage;
use tactix::{Actor, Ctx, Handler};

use crate::crypto::Keypair;
use crate::messages::{KeypunkdRequest, KeypunkdResponse};
use crate::seed_store::SeedStore;
use crate::usecases;

/// Convenience bound for a thread-safe seed store usable inside an actor.
pub trait Storage: SeedStore + Send + Sync + 'static {}
impl<T: SeedStore + Send + Sync + 'static> Storage for T {}

pub struct Dispatcher<S: Storage> {
    keystore: Keypair,
    seed_store: S,
    session: Option<[u8; 32]>,
    skip_session_auth: bool,
}

impl<S: Storage> Dispatcher<S> {
    pub fn new(keystore: Keypair, seed_store: S) -> Self {
        Self {
            keystore,
            seed_store,
            session: None,
            skip_session_auth: false,
        }
    }

    /// When enabled, all session authentication checks are skipped.
    /// In-process messages (`sender_public_key` is `None`) are still rejected
    /// unless this mode is active.
    pub fn with_skip_session_auth(mut self, skip: bool) -> Self {
        self.skip_session_auth = skip;
        self
    }

    /// Returns the verified sender public key, or bails if the message
    /// is in-process and session auth is enabled.
    fn verify_message(&self, msg: &IpcMessage) -> Result<(), String> {
        if self.skip_session_auth {
            return Ok(());
        }

        msg.sender_public_key
            .ok_or_else(|| "rejecting in-process message: no sender public key".to_string())?;

        Ok(())
    }

    /// Sets the active session from the message's sender public key.
    fn set_session(&mut self, msg: &IpcMessage) {
        if let Some(pk) = msg.sender_public_key {
            self.session = Some(pk);
        }
    }
}

impl<S: Storage> Actor for Dispatcher<S> {}

impl<S: Storage> Handler<IpcMessage> for Dispatcher<S> {
    async fn handle(&mut self, msg: IpcMessage, _ctx: &Ctx<Self>) -> Result<Vec<u8>, String> {
        self.verify_message(&msg)?;

        let request: KeypunkdRequest =
            postcard::from_bytes(&msg.payload).map_err(|e| format!("deserialize error: {e}"))?;

        let response = match request {
            // Always allowed — no session check.
            KeypunkdRequest::GetPublicKey => KeypunkdResponse::PublicKey {
                key: self.keystore.public_key(),
            },
            // Password-authenticated — sets session on success.
            KeypunkdRequest::GenerateSeed {
                encrypted_password,
                client_public_key,
            } => match usecases::generate_seed(
                &self.keystore,
                &encrypted_password,
                &client_public_key,
                &self.seed_store,
            ) {
                Ok(encrypted_mnemonic) => {
                    self.set_session(&msg);
                    KeypunkdResponse::SeedGenerated { encrypted_mnemonic }
                }
                Err(e) => KeypunkdResponse::Error {
                    message: e.to_string(),
                },
            },
        };

        postcard::to_allocvec(&response).map_err(|e| format!("serialize error: {e}"))
    }
}
