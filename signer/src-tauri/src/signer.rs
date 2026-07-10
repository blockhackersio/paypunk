use keypunkd::crypto::Keypair;
use keypunkd::keypunk::Keypunk;
use keypunkd::protocol::ProtocolService;
use keypunkd::seed_store::FilesystemSeedStore;
use paypunk_chains_ethereum::signer::EthereumSignerProtocol;
use paypunk_chains_zcash::signer::ZcashSignerProtocol;
use paypunk_chains_zcash::to_local_params;
use paypunk_types::{
    ArtifactSummary, KeypunkdRequest, KeypunkdResponse, ProtocolId,
};
use std::path::PathBuf;
use zcash_protocol::consensus::{Network, NetworkType};
use zeroize::Zeroizing;
use base64::Engine;
use rand::RngCore;

pub struct SignerState {
    keypunk: Keypunk<FilesystemSeedStore>,
    client_keypair: Keypair,
    server_public_key: [u8; 32],
    status: SignerStatus,
    password: Option<String>,
    data_dir: PathBuf,
}

pub enum SignerStatus {
    Idle,
    Previewing {
        raw_artifact: Vec<u8>,
        summary: ArtifactSummary,
        derivation_path: String,
        protocol: ProtocolId,
        preview_signature: Vec<u8>,
    },
    Signing,
    Signed {
        signed_artifact: Vec<u8>,
    },
    Error(String),
}

impl SignerState {
    pub fn create(data_dir: PathBuf) -> Self {
        let server_keypair = Keypair::new();
        let server_public_key = server_keypair.public_key();
        let client_keypair = Keypair::new();

        let seed_store = FilesystemSeedStore::new(
            data_dir.join("seed.enc").into_boxed_path(),
        );

        let mut protocols = ProtocolService::new();
        let (params, network_type) = (Network::TestNetwork, NetworkType::Regtest);
        protocols.register(
            ProtocolId::Zcash,
            Box::new(ZcashSignerProtocol::new(
                to_local_params(params, network_type),
                network_type,
            )),
        );
        protocols.register(
            ProtocolId::Ethereum,
            Box::new(EthereumSignerProtocol::new()),
        );

        let keypunk = Keypunk::new(server_keypair, seed_store, protocols);
        let password = Self::load_password(&data_dir);

        Self {
            keypunk,
            client_keypair,
            server_public_key,
            status: SignerStatus::Idle,
            password,
            data_dir,
        }
    }

    fn password_path(&self) -> PathBuf {
        self.data_dir.join(".seed-password")
    }

    fn load_password(data_dir: &std::path::Path) -> Option<String> {
        let path = data_dir.join(".seed-password");
        std::fs::read_to_string(path).ok()
    }

    fn ensure_password(&mut self) -> Result<String, String> {
        if let Some(ref pwd) = self.password {
            return Ok(pwd.clone());
        }
        let mut bytes = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut bytes);
        let password = base64::engine::general_purpose::STANDARD.encode(&bytes);

        let path = self.password_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, &password).map_err(|e| e.to_string())?;

        self.password = Some(password.clone());
        Ok(password)
    }

    pub fn has_seed(&self) -> bool {
        let response = self.keypunk.handle_request(
            KeypunkdRequest::HasSeed,
            Some(self.client_keypair.public_key()),
        );
        match response {
            KeypunkdResponse::HasSeed { exists } => exists,
            _ => false,
        }
    }

    pub fn generate_seed(&mut self) -> Result<String, String> {
        let password = self.ensure_password()?;
        let client_pk = self.client_keypair.public_key();

        let encrypted_password =
            self.client_keypair
                .encrypt(Zeroizing::new(password), &self.server_public_key);

        let request = KeypunkdRequest::GenerateSeed {
            encrypted_password,
            client_public_key: client_pk,
        };

        let response = self.keypunk.handle_request(request, Some(client_pk));

        match response {
            KeypunkdResponse::SeedGenerated {
                encrypted_mnemonic,
            } => {
                let mnemonic = self
                    .client_keypair
                    .decrypt(&encrypted_mnemonic, &self.server_public_key)
                    .map_err(|e| format!("decrypt mnemonic failed: {e}"))?;
                Ok(mnemonic.to_string())
            }
            KeypunkdResponse::Error { message } => Err(message),
            _ => Err("unexpected response".to_string()),
        }
    }

    pub fn handle_request(&mut self, request_bytes: &[u8]) -> Vec<u8> {
        let request: KeypunkdRequest = match postcard::from_bytes(request_bytes) {
            Ok(r) => r,
            Err(e) => {
                let resp = KeypunkdResponse::Error {
                    message: format!("deserialize failed: {e}"),
                };
                return postcard::to_allocvec(&resp).unwrap_or_default();
            }
        };

        let client_pk = self.client_keypair.public_key();

        let (derivation_path, protocol) = match &request {
            KeypunkdRequest::PreviewArtifact {
                derivation_path,
                protocol,
                ..
            } => (Some(derivation_path.clone()), Some(*protocol)),
            _ => (None, None),
        };

        let response = self.keypunk.handle_request(request, Some(client_pk));

        if let KeypunkdResponse::ArtifactPreview {
            ref raw_artifact,
            ref parsed_summary,
            ref signature,
            ..
        } = response
        {
            let summary: ArtifactSummary = match postcard::from_bytes(parsed_summary) {
                Ok(s) => s,
                Err(_) => {
                    let resp = KeypunkdResponse::Error {
                        message: "summary deserialize failed".to_string(),
                    };
                    return postcard::to_allocvec(&resp).unwrap_or_default();
                }
            };

            self.status = SignerStatus::Previewing {
                raw_artifact: raw_artifact.clone(),
                summary,
                derivation_path: derivation_path.unwrap_or_default(),
                protocol: protocol.unwrap_or(ProtocolId::Zcash),
                preview_signature: signature.clone(),
            };
        }

        postcard::to_allocvec(&response).unwrap_or_default()
    }

    pub fn approve_and_sign(&mut self) -> Result<Vec<u8>, String> {
        let (raw_artifact, derivation_path, preview_signature) = match &self.status {
            SignerStatus::Previewing {
                raw_artifact,
                derivation_path,
                preview_signature,
                ..
            } => (
                raw_artifact.clone(),
                derivation_path.clone(),
                preview_signature.clone(),
            ),
            _ => return Err("no preview to sign".to_string()),
        };

        self.status = SignerStatus::Signing;

        let password = self
            .password
            .as_ref()
            .ok_or("no password set — generate seed first")?;
        let client_pk = self.client_keypair.public_key();

        let mut plaintext = Vec::new();
        plaintext.extend_from_slice(&(raw_artifact.len() as u32).to_le_bytes());
        plaintext.extend_from_slice(&raw_artifact);
        plaintext.extend_from_slice(&(preview_signature.len() as u32).to_le_bytes());
        plaintext.extend_from_slice(&preview_signature);
        plaintext.extend_from_slice(password.as_bytes());

        let encrypted_payload =
            self.client_keypair
                .encrypt_bytes(&plaintext, &self.server_public_key);

        let request = KeypunkdRequest::AuthorizeArtifact {
            encrypted_payload,
            ephemeral_public_key: client_pk,
            derivation_path,
        };

        let response = self.keypunk.handle_request(request, Some(client_pk));

        match response {
            KeypunkdResponse::ArtifactAuthorized { signed_artifact } => {
                self.status = SignerStatus::Signed {
                    signed_artifact: signed_artifact.clone(),
                };
                Ok(signed_artifact)
            }
            KeypunkdResponse::Error { message } => {
                self.status = SignerStatus::Error(message.clone());
                Err(message)
            }
            _ => {
                let msg = "unexpected response".to_string();
                self.status = SignerStatus::Error(msg.clone());
                Err(msg)
            }
        }
    }

    pub fn status(&self) -> &SignerStatus {
        &self.status
    }

    pub fn status_mut(&mut self) -> &mut SignerStatus {
        &mut self.status
    }

    pub fn mnemonic(&self) -> Option<&str> {
        None
    }
}
