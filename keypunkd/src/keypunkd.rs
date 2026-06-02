use paypunk_ipc::IpcMessage;
use tactix::{Actor, Ctx, Handler};
use tracing::{debug, info, warn};
use zeroize::Zeroize;

use crate::crypto::Keypair;
use crate::messages::{KeypunkdRequest, KeypunkdResponse};
use crate::seed_store::SeedStore;
use crate::usecases;

/// Convenience bound for a thread-safe seed store usable inside an actor.
pub trait Storage: SeedStore + Send + Sync + 'static {}
impl<T: SeedStore + Send + Sync + 'static> Storage for T {}

/// An unlocked session holding the decrypted seed in memory.
struct Session {
    peer_pk: [u8; 32],
    seed: [u8; 64],
}

impl Session {
    fn new(peer_pk: [u8; 32], seed: [u8; 64]) -> Self {
        Self { peer_pk, seed }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.seed.zeroize();
    }
}

pub struct Keypunkd<S: Storage> {
    keystore: Keypair,
    seed_store: S,
    session: Option<Session>,
    skip_session_auth: bool,
}

impl<S: Storage> Keypunkd<S> {
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

    /// Ensures there is an active unlocked session for the given sender.
    fn require_session(&self, msg: &IpcMessage) -> Result<&Session, String> {
        if self.skip_session_auth {
            return self
                .session
                .as_ref()
                .ok_or_else(|| "no active session — call Unlock first".to_string());
        }
        let sender_pk = msg
            .sender_public_key
            .ok_or_else(|| "in-process message has no sender key".to_string())?;
        self.session
            .as_ref()
            .filter(|s| s.peer_pk == sender_pk)
            .ok_or_else(|| "no active session — call Unlock first".to_string())
    }

    /// Sets the active session from the message's sender public key and seed.
    fn set_session(&mut self, msg: &IpcMessage, seed: [u8; 64]) {
        if self.skip_session_auth {
            self.session = Some(Session::new([0u8; 32], seed));
        } else if let Some(pk) = msg.sender_public_key {
            self.session = Some(Session::new(pk, seed));
        }
    }

    /// Clears the active session, zeroizing the seed.
    fn clear_session(&mut self) {
        self.session = None;
    }
}

impl<S: Storage> Actor for Keypunkd<S> {}

impl<S: Storage> Handler<IpcMessage> for Keypunkd<S> {
    async fn handle(&mut self, msg: IpcMessage, _ctx: &Ctx<Self>) -> Result<Vec<u8>, String> {
        self.verify_message(&msg)?;

        let request: KeypunkdRequest =
            postcard::from_bytes(&msg.payload).map_err(|e| format!("deserialize error: {e}"))?;

        debug!(?request, "dispatching request");

        let response = match request {
            // Always allowed — no session check.
            KeypunkdRequest::GetPublicKey => {
                info!("handling GetPublicKey");
                KeypunkdResponse::PublicKey {
                    key: self.keystore.public_key(),
                }
            }
            // Password-authenticated — sets session on success.
            KeypunkdRequest::GenerateSeed {
                encrypted_password,
                client_public_key,
            } => {
                info!("handling GenerateSeed");
                match usecases::generate_seed(
                    &self.keystore,
                    &encrypted_password,
                    &client_public_key,
                    &self.seed_store,
                ) {
                    Ok(encrypted_mnemonic) => {
                        info!("seed generated successfully");
                        KeypunkdResponse::SeedGenerated { encrypted_mnemonic }
                    }
                    Err(e) => {
                        warn!(error = %e, "GenerateSeed failed");
                        KeypunkdResponse::Error {
                            message: e.to_string(),
                        }
                    }
                }
            }
            // Password-authenticated — sets session on success.
            KeypunkdRequest::RestoreSeed {
                encrypted_mnemonic,
                encrypted_password,
                client_public_key,
            } => {
                info!("handling RestoreSeed");
                match usecases::restore_seed(
                    &self.keystore,
                    &encrypted_mnemonic,
                    &encrypted_password,
                    &client_public_key,
                    &self.seed_store,
                ) {
                    Ok(()) => {
                        info!("seed restored successfully");
                        KeypunkdResponse::SeedRestored
                    }
                    Err(e) => {
                        warn!(error = %e, "RestoreSeed failed");
                        KeypunkdResponse::Error {
                            message: e.to_string(),
                        }
                    }
                }
            }
            // Password-authenticated — decrypts seed, holds in memory.
            KeypunkdRequest::Unlock {
                encrypted_password,
                client_public_key,
            } => {
                info!("handling Unlock");
                let client_pk = &client_public_key;
                match usecases::decrypt_seed(
                    &encrypted_password,
                    client_pk,
                    &self.keystore,
                    &self.seed_store,
                ) {
                    Ok(seed) => {
                        self.set_session(&msg, seed);
                        info!("wallet unlocked");
                        KeypunkdResponse::Unlocked
                    }
                    Err(e) => {
                        warn!(error = %e, "Unlock failed");
                        KeypunkdResponse::Error { message: e }
                    }
                }
            }
            // Requires active session.
            KeypunkdRequest::DeriveAddress { index } => {
                info!("handling DeriveAddress");
                match self.require_session(&msg) {
                    Ok(session) => match usecases::derive_address(&session.seed, index) {
                        Ok(address) => {
                            debug!(index, %address, "address derived");
                            KeypunkdResponse::AddressDerived { address }
                        }
                        Err(e) => {
                            warn!(error = %e, "DeriveAddress failed");
                            KeypunkdResponse::Error { message: e }
                        }
                    },
                    Err(e) => KeypunkdResponse::Error { message: e },
                }
            }
            // Clears session.
            KeypunkdRequest::Lock => {
                info!("handling Lock");
                self.clear_session();
                KeypunkdResponse::Locked
            }
        };

        let encoded =
            postcard::to_allocvec(&response).map_err(|e| format!("serialize error: {e}"))?;
        debug!(response_len = encoded.len(), "sending response");
        Ok(encoded)
    }
}
