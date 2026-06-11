use paypunk_ipc::IpcMessage;
use paypunk_types::ProtocolId;
use tactix::{Actor, Ctx, Handler};
use tracing::{debug, info, warn};
use zeroize::Zeroize;
use blake2::Digest;

use crate::crypto::Keypair;
use crate::key;
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

    fn lock(&mut self) -> KeypunkdResponse {
        info!("handling Lock");
        self.clear_session();
        KeypunkdResponse::Locked
    }

    fn preview_artifact(
        &self,
        raw_artifact: Vec<u8>,
        protocol: ProtocolId,
        msg: &IpcMessage,
    ) -> KeypunkdResponse {
        info!(?protocol, "handling PreviewArtifact");
        let session = match self.require_session(msg) {
            Ok(s) => s,
            Err(e) => return KeypunkdResponse::Error { message: e },
        };

        // Parse the artifact into a summary
        let parsed_summary = match usecases::preview_artifact(&self.protocols, protocol, &raw_artifact) {
            Ok(s) => s,
            Err(e) => return KeypunkdResponse::Error { message: e },
        };

        // Sign H(raw, parsed) with keypunkd's keypair for WYSIWYS verification
        let mut to_sign = Vec::new();
        to_sign.extend_from_slice(&raw_artifact);
        to_sign.extend_from_slice(&parsed_summary);
        let hash = blake2::Blake2b::<blake2::digest::consts::U32>::digest(&to_sign);
        let signature = self.keystore.encrypt_bytes(&hash, &session.peer_pk);

        KeypunkdResponse::ArtifactPreview {
            raw_artifact,
            parsed_summary,
            signature,
            keypunkd_public_key: self.keystore.public_key(),
        }
    }

    fn authorize_artifact(
        &self,
        encrypted_payload: Vec<u8>,
        ephemeral_public_key: [u8; 32],
        msg: &IpcMessage,
    ) -> KeypunkdResponse {
        info!("handling AuthorizeArtifact");
        let _session = match self.require_session(msg) {
            Ok(s) => s,
            Err(e) => return KeypunkdResponse::Error { message: e },
        };

        // Decrypt the payload: (raw, sig, pw) encrypted with ephemeral key
        let plaintext = match self
            .keystore
            .decrypt_bytes(&encrypted_payload, &ephemeral_public_key)
        {
            Ok(p) => p,
            Err(e) => return KeypunkdResponse::Error { message: format!("decryption failed: {e}") },
        };

        // Parse: first 64 bytes = raw_artifact length prefix + raw, then sig, then pw
        // Format: raw_len(4) + raw(raw_len) + sig_len(4) + sig(sig_len) + pw(rest)
        if plaintext.len() < 8 {
            return KeypunkdResponse::Error {
                message: "invalid encrypted payload".to_string(),
            };
        }

        let raw_len = u32::from_le_bytes(plaintext[0..4].try_into().unwrap()) as usize;
        if plaintext.len() < 4 + raw_len + 4 {
            return KeypunkdResponse::Error {
                message: "invalid encrypted payload: truncated".to_string(),
            };
        }
        let raw_end = 4 + raw_len;
        let raw_artifact = &plaintext[4..raw_end];

        let sig_len = u32::from_le_bytes(plaintext[raw_end..raw_end + 4].try_into().unwrap()) as usize;
        let sig_start = raw_end + 4;
        let sig_end = sig_start + sig_len;
        if plaintext.len() < sig_end {
            return KeypunkdResponse::Error {
                message: "invalid encrypted payload: truncated sig".to_string(),
            };
        }
        let sig = &plaintext[sig_start..sig_end];
        let password_bytes = &plaintext[sig_end..];

        // Re-parse the raw artifact to verify it matches
        // Try each protocol
        let parsed_summary = self.try_parse_artifact(raw_artifact);

        // Verify the signature over H(raw, parsed)
        if let Ok(ref summary) = parsed_summary {
            let mut to_verify = Vec::new();
            to_verify.extend_from_slice(raw_artifact);
            to_verify.extend_from_slice(summary);
            let hash = blake2::Blake2b::<blake2::digest::consts::U32>::digest(&to_verify);

            // Decrypt the signature to get the expected hash
            let decrypted_sig = match self.keystore.decrypt_bytes(sig, &ephemeral_public_key) {
                Ok(d) => d,
                Err(_) => {
                    return KeypunkdResponse::Error {
                        message: "signature verification failed: cannot decrypt".to_string(),
                    }
                }
            };

            if decrypted_sig.as_slice() != hash.as_slice() {
                return KeypunkdResponse::Error {
                    message: "artifact verification failed: summary mismatch".to_string(),
                };
            }
        }

        // Decrypt seed with password
        let password_str = match String::from_utf8(password_bytes.to_vec()) {
            Ok(s) => s,
            Err(_) => {
                return KeypunkdResponse::Error {
                    message: "invalid password encoding".to_string(),
                }
            }
        };

        let encrypted_store = match self.seed_store.read() {
            Ok(Some(e)) => e,
            Ok(None) => {
                return KeypunkdResponse::Error {
                    message: "no seed found".to_string(),
                }
            }
            Err(e) => {
                return KeypunkdResponse::Error {
                    message: format!("read seed failed: {e}"),
                }
            }
        };

        let seed = match key::decrypt_seed(&encrypted_store, &password_str) {
            Ok(s) => s,
            Err(e) => {
                return KeypunkdResponse::Error {
                    message: format!("seed decryption failed: {e}"),
                }
            }
        };

        // Sign the artifact
        let signed_artifact = match usecases::sign_artifact(&seed, &self.protocols, raw_artifact) {
            Ok(s) => s,
            Err(e) => return KeypunkdResponse::Error { message: e },
        };

        KeypunkdResponse::ArtifactAuthorized { signed_artifact }
    }

    fn try_parse_artifact(&self, raw_artifact: &[u8]) -> Result<Vec<u8>, String> {
        for id in [
            ProtocolId::Zcash,
            ProtocolId::Ethereum,
            ProtocolId::Bitcoin,
            ProtocolId::Monero,
            ProtocolId::Solana,
        ] {
            if let Some(deriver) = self.protocols.get(id) {
                if let Ok(summary) = deriver.parse_artifact(raw_artifact) {
                    return Ok(summary);
                }
            }
        }
        Err("no protocol could parse the artifact".to_string())
    }

    fn export_viewing_key(
        &self,
        msg: &IpcMessage,
        protocol: ProtocolId,
        account: u32,
    ) -> KeypunkdResponse {
        info!(?protocol, account, "handling ExportViewingKey");
        let session = match self.require_session(msg) {
            Ok(s) => s,
            Err(e) => return KeypunkdResponse::Error { message: e },
        };
        self.respond(
            "export_viewing_key",
            usecases::export_viewing_key(&session.seed, &self.protocols, protocol, account),
            |key| KeypunkdResponse::ViewingKey { key },
        )
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
            KeypunkdRequest::Lock => self.lock(),
            KeypunkdRequest::PreviewArtifact {
                raw_artifact,
                protocol,
            } => self.preview_artifact(raw_artifact, protocol, &msg),
            KeypunkdRequest::AuthorizeArtifact {
                encrypted_payload,
                ephemeral_public_key,
            } => self.authorize_artifact(encrypted_payload, ephemeral_public_key, &msg),
            KeypunkdRequest::ExportViewingKey { protocol, account } => {
                self.export_viewing_key(&msg, protocol, account)
            }
        };

        let encoded =
            postcard::to_allocvec(&response).map_err(|e| format!("serialize error: {e}"))?;
        debug!(response_len = encoded.len(), "sending response");
        Ok(encoded)
    }
}
