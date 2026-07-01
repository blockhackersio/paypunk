use blake2::Digest;
use paypunk_ipc::IpcMessage;
use paypunk_types::ProtocolId;
use tactix::{Actor, Ctx, Handler};
use tracing::{debug, info, warn};

use crate::crypto::Keypair;
use crate::key;
use crate::messages::{KeypunkdRequest, KeypunkdResponse};
use crate::protocol::ProtocolService;
use crate::seed_store::SeedStore;
use crate::usecases;

/// Convenience bound for a thread-safe seed store usable inside an actor.
pub trait Storage: SeedStore + Send + Sync + 'static {}
impl<T: SeedStore + Send + Sync + 'static> Storage for T {}

pub struct Keypunkd<S: Storage> {
    keystore: Keypair,
    seed_store: S,
    protocols: ProtocolService,
}

impl<S: Storage> Keypunkd<S> {
    pub fn new(keystore: Keypair, seed_store: S, protocols: ProtocolService) -> Self {
        Self {
            keystore,
            seed_store,
            protocols,
        }
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

    fn preview_artifact(
        &self,
        raw_artifact: Vec<u8>,
        protocol: ProtocolId,
        derivation_path: String,
        msg: &IpcMessage,
    ) -> KeypunkdResponse {
        info!(?protocol, "handling PreviewArtifact");

        let parsed_summary =
            match usecases::preview_artifact(&self.protocols, protocol, &raw_artifact) {
                Ok(s) => s,
                Err(e) => return KeypunkdResponse::Error { message: e },
            };

        // Sign H(raw, parsed, path) with keypunkd's keypair for WYSIWYS verification
        let mut to_sign = Vec::new();
        to_sign.extend_from_slice(&raw_artifact);
        to_sign.extend_from_slice(&parsed_summary);
        to_sign.extend_from_slice(derivation_path.as_bytes());
        let hash = blake2::Blake2b::<blake2::digest::consts::U32>::digest(&to_sign);

        // Encrypt the attestation to the client's public key so only they can decrypt it
        let peer_pk = msg.sender_public_key.unwrap_or([0u8; 32]);
        let signature = self.keystore.encrypt_bytes(&hash, &peer_pk);

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
        derivation_path: String,
        sender_public_key: Option<[u8; 32]>,
    ) -> KeypunkdResponse {
        info!("handling AuthorizeArtifact");

        // Decrypt the payload: (raw, sig, pw) encrypted with ephemeral key
        let plaintext = match self
            .keystore
            .decrypt_bytes(&encrypted_payload, &ephemeral_public_key)
        {
            Ok(p) => p,
            Err(e) => {
                return KeypunkdResponse::Error {
                    message: format!("decryption failed: {e}"),
                }
            }
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

        let sig_len =
            u32::from_le_bytes(plaintext[raw_end..raw_end + 4].try_into().unwrap()) as usize;
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
        let parsed_summary = self.try_parse_artifact(raw_artifact);

        // Verify the signature over H(raw, parsed, path)
        if let Ok(ref summary) = parsed_summary {
            let mut to_verify = Vec::new();
            to_verify.extend_from_slice(raw_artifact);
            to_verify.extend_from_slice(summary);
            to_verify.extend_from_slice(derivation_path.as_bytes());
            let hash = blake2::Blake2b::<blake2::digest::consts::U32>::digest(&to_verify);

            let peer_pk = sender_public_key.unwrap_or([0u8; 32]);
            let decrypted_sig = match self.keystore.decrypt_bytes(sig, &peer_pk) {
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
        let signed_artifact =
            match usecases::sign_artifact(&seed, &self.protocols, &derivation_path, raw_artifact) {
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
        encrypted_password: Vec<u8>,
        client_public_key: [u8; 32],
        protocol: ProtocolId,
        derivation_path: String,
    ) -> KeypunkdResponse {
        info!(?protocol, path = %derivation_path, "handling ExportViewingKey");

        let seed = match usecases::decrypt_seed(
            &encrypted_password,
            &client_public_key,
            &self.keystore,
            &self.seed_store,
        ) {
            Ok(s) => s,
            Err(e) => return KeypunkdResponse::Error { message: e },
        };

        self.respond(
            "export_viewing_key",
            usecases::export_viewing_key(&seed, &self.protocols, protocol, &derivation_path),
            |key| KeypunkdResponse::ViewingKey { key },
        )
    }

    fn has_seed(&self) -> KeypunkdResponse {
        info!("handling HasSeed");
        let exists = self.seed_store.read().ok().flatten().is_some();
        KeypunkdResponse::HasSeed { exists }
    }

    fn verify_password(
        &self,
        encrypted_password: Vec<u8>,
        client_public_key: [u8; 32],
    ) -> KeypunkdResponse {
        info!("handling VerifyPassword");
        match usecases::decrypt_seed(
            &encrypted_password,
            &client_public_key,
            &self.keystore,
            &self.seed_store,
        ) {
            Ok(_) => KeypunkdResponse::PasswordVerified,
            Err(e) => KeypunkdResponse::Error { message: e },
        }
    }

    fn bulk_export_viewing_keys(
        &self,
        encrypted_password: Vec<u8>,
        client_public_key: [u8; 32],
        paths: Vec<(ProtocolId, String)>,
    ) -> KeypunkdResponse {
        info!(path_count = paths.len(), "handling BulkExportViewingKeys");

        let seed = match usecases::decrypt_seed(
            &encrypted_password,
            &client_public_key,
            &self.keystore,
            &self.seed_store,
        ) {
            Ok(s) => s,
            Err(e) => return KeypunkdResponse::Error { message: e },
        };

        self.respond(
            "bulk_export_viewing_keys",
            usecases::bulk_export_viewing_keys(&seed, &self.protocols, &paths),
            |keys| KeypunkdResponse::ViewingKeys { keys },
        )
    }
}

impl<S: Storage> Actor for Keypunkd<S> {}

impl<S: Storage> Handler<IpcMessage> for Keypunkd<S> {
    async fn handle(&mut self, msg: IpcMessage, _ctx: &Ctx<Self>) -> Result<Vec<u8>, String> {
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
            KeypunkdRequest::PreviewArtifact {
                raw_artifact,
                protocol,
                derivation_path,
            } => self.preview_artifact(raw_artifact, protocol, derivation_path, &msg),
            KeypunkdRequest::AuthorizeArtifact {
                encrypted_payload,
                ephemeral_public_key,
                derivation_path,
            } => self.authorize_artifact(
                encrypted_payload,
                ephemeral_public_key,
                derivation_path,
                msg.sender_public_key,
            ),
            KeypunkdRequest::ExportViewingKey {
                encrypted_password,
                client_public_key,
                protocol,
                derivation_path,
            } => {
                self.export_viewing_key(encrypted_password, client_public_key, protocol, derivation_path)
            }
            KeypunkdRequest::HasSeed => self.has_seed(),
            KeypunkdRequest::VerifyPassword {
                encrypted_password,
                client_public_key,
            } => self.verify_password(encrypted_password, client_public_key),
            KeypunkdRequest::BulkExportViewingKeys {
                encrypted_password,
                client_public_key,
                paths,
            } => self.bulk_export_viewing_keys(encrypted_password, client_public_key, paths),
        };

        let encoded =
            postcard::to_allocvec(&response).map_err(|e| format!("serialize error: {e}"))?;
        debug!(response_len = encoded.len(), "sending response");
        Ok(encoded)
    }
}
