use paypunk_ipc::IpcMessage;
use paypunk_types::ProtocolId;
use tactix::{Actor, Ctx, Handler};
use tracing::{debug, info, warn};
use zeroize::Zeroize;

use crate::crypto::Keypair;
use crate::messages::{KeypunkdRequest, KeypunkdResponse};
use crate::protocol::ProtocolService;
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
    protocols: ProtocolService,
    session: Option<Session>,
    skip_session_auth: bool,
}

impl<S: Storage> Keypunkd<S> {
    pub fn new(keystore: Keypair, seed_store: S, protocols: ProtocolService) -> Self {
        Self {
            keystore,
            seed_store,
            protocols,
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

    fn respond<T>(
        &self,
        label: &str,
        result: Result<T, String>,
        map_ok: impl FnOnce(T) -> KeypunkdResponse,
    ) -> KeypunkdResponse {
        match result {
            Ok(v) => map_ok(v),
            Err(e) => {
                warn!(error = %e, "{label} failed");
                KeypunkdResponse::Error { message: e }
            }
        }
    }

    fn get_encryption_key(&self) -> KeypunkdResponse {
        info!("handling GetEncryptionKey");
        KeypunkdResponse::EncryptionKey {
            key: self.keystore.public_key(),
        }
    }

    fn generate_seed(
        &self,
        encrypted_password: Vec<u8>,
        client_public_key: [u8; 32],
    ) -> KeypunkdResponse {
        info!("handling GenerateSeed");
        self.respond(
            "generate_seed",
            usecases::generate_seed(
                &self.keystore,
                &encrypted_password,
                &client_public_key,
                &self.seed_store,
            )
            .map_err(|e| e.to_string()),
            |encrypted_mnemonic| KeypunkdResponse::SeedGenerated { encrypted_mnemonic },
        )
    }

    fn restore_seed(
        &self,
        encrypted_mnemonic: Vec<u8>,
        encrypted_password: Vec<u8>,
        client_public_key: [u8; 32],
    ) -> KeypunkdResponse {
        info!("handling RestoreSeed");
        self.respond(
            "restore_seed",
            usecases::restore_seed(
                &self.keystore,
                &encrypted_mnemonic,
                &encrypted_password,
                &client_public_key,
                &self.seed_store,
            )
            .map_err(|e| e.to_string()),
            |()| KeypunkdResponse::SeedRestored,
        )
    }

    fn unlock(
        &mut self,
        msg: &IpcMessage,
        encrypted_password: Vec<u8>,
        client_public_key: [u8; 32],
    ) -> KeypunkdResponse {
        info!("handling Unlock");
        let client_pk = &client_public_key;
        match usecases::decrypt_seed(
            &encrypted_password,
            client_pk,
            &self.keystore,
            &self.seed_store,
        ) {
            Ok(seed) => {
                self.set_session(msg, seed);
                info!("wallet unlocked");
                KeypunkdResponse::Unlocked
            }
            Err(e) => {
                warn!(error = %e, "unlock failed");
                KeypunkdResponse::Error { message: e }
            }
        }
    }

    fn derive_public_key(
        &self,
        msg: &IpcMessage,
        protocol: ProtocolId,
        account: u32,
    ) -> KeypunkdResponse {
        info!(?protocol, account, "handling DerivePublicKey");
        let session = match self.require_session(msg) {
            Ok(s) => s,
            Err(e) => return KeypunkdResponse::Error { message: e },
        };
        self.respond(
            "derive_public_key",
            usecases::derive_public_key(&session.seed, &self.protocols, protocol, account),
            |key| KeypunkdResponse::ProtocolPublicKey { key },
        )
    }

    fn sign(
        &self,
        msg: &IpcMessage,
        protocol: ProtocolId,
        account: u32,
        payload: Vec<u8>,
    ) -> KeypunkdResponse {
        info!(?protocol, account, "handling Sign");
        let session = match self.require_session(msg) {
            Ok(s) => s,
            Err(e) => return KeypunkdResponse::Error { message: e },
        };
        self.respond(
            "sign",
            usecases::sign(&session.seed, &self.protocols, protocol, account, &payload),
            |signature| KeypunkdResponse::Signature { signature },
        )
    }

    fn lock(&mut self) -> KeypunkdResponse {
        info!("handling Lock");
        self.clear_session();
        KeypunkdResponse::Locked
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
            KeypunkdRequest::GetEncryptionKey => self.get_encryption_key(),
            KeypunkdRequest::GenerateSeed {
                encrypted_password,
                client_public_key,
            } => self.generate_seed(encrypted_password, client_public_key),
            KeypunkdRequest::RestoreSeed {
                encrypted_mnemonic,
                encrypted_password,
                client_public_key,
            } => self.restore_seed(encrypted_mnemonic, encrypted_password, client_public_key),
            KeypunkdRequest::Unlock {
                encrypted_password,
                client_public_key,
            } => self.unlock(&msg, encrypted_password, client_public_key),
            KeypunkdRequest::DerivePublicKey { protocol, account } => {
                self.derive_public_key(&msg, protocol, account)
            }
            KeypunkdRequest::Sign {
                protocol,
                account,
                payload,
            } => self.sign(&msg, protocol, account, payload),
            KeypunkdRequest::Lock => self.lock(),
        };

        let encoded =
            postcard::to_allocvec(&response).map_err(|e| format!("serialize error: {e}"))?;
        debug!(response_len = encoded.len(), "sending response");
        Ok(encoded)
    }
}
