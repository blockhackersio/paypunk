use paypunk_types::ProtocolId;

use crate::messages::{KeypunkdRequest, KeypunkdResponse};
use paypunk_ipc::IpcMessage;
use tactix::{Recipient, Sender};

pub struct KeypunkService {
    recipient: Recipient<IpcMessage>,
}

impl KeypunkService {
    pub fn new(recipient: Recipient<IpcMessage>) -> Self {
        Self { recipient }
    }

    async fn send(&self, request: KeypunkdRequest) -> Result<KeypunkdResponse, String> {
        let payload =
            postcard::to_allocvec(&request).map_err(|e| format!("serialize error: {e}"))?;
        let msg = IpcMessage::new(payload);
        let response_bytes = self.recipient.ask(msg).await?;
        postcard::from_bytes(&response_bytes).map_err(|e| format!("deserialize error: {e}"))
    }

    pub async fn get_encryption_key(&self) -> Result<[u8; 32], String> {
        match self.send(KeypunkdRequest::GetEncryptionKey).await? {
            KeypunkdResponse::EncryptionKey { key } => Ok(key),
            KeypunkdResponse::Error { message } => Err(message),
            _ => Err("unexpected response variant".to_string()),
        }
    }

    pub async fn generate_seed(
        &self,
        encrypted_password: Vec<u8>,
        client_public_key: [u8; 32],
    ) -> Result<Vec<u8>, String> {
        match self
            .send(KeypunkdRequest::GenerateSeed {
                encrypted_password,
                client_public_key,
            })
            .await?
        {
            KeypunkdResponse::SeedGenerated { encrypted_mnemonic } => Ok(encrypted_mnemonic),
            KeypunkdResponse::Error { message } => Err(message),
            _ => Err("unexpected response variant".to_string()),
        }
    }

    pub async fn restore_seed(
        &self,
        encrypted_mnemonic: Vec<u8>,
        encrypted_password: Vec<u8>,
        client_public_key: [u8; 32],
    ) -> Result<(), String> {
        match self
            .send(KeypunkdRequest::RestoreSeed {
                encrypted_mnemonic,
                encrypted_password,
                client_public_key,
            })
            .await?
        {
            KeypunkdResponse::SeedRestored => Ok(()),
            KeypunkdResponse::Error { message } => Err(message),
            _ => Err("unexpected response variant".to_string()),
        }
    }

    pub async fn unlock(
        &self,
        encrypted_password: Vec<u8>,
        client_public_key: [u8; 32],
    ) -> Result<(), String> {
        match self
            .send(KeypunkdRequest::Unlock {
                encrypted_password,
                client_public_key,
            })
            .await?
        {
            KeypunkdResponse::Unlocked => Ok(()),
            KeypunkdResponse::Error { message } => Err(message),
            _ => Err("unexpected response variant".to_string()),
        }
    }

    pub async fn lock(&self) -> Result<(), String> {
        match self.send(KeypunkdRequest::Lock).await? {
            KeypunkdResponse::Locked => Ok(()),
            KeypunkdResponse::Error { message } => Err(message),
            _ => Err("unexpected response variant".to_string()),
        }
    }

    pub async fn preview_artifact(
        &self,
        raw_artifact: Vec<u8>,
        protocol: ProtocolId,
    ) -> Result<KeypunkdResponse, String> {
        self.send(KeypunkdRequest::PreviewArtifact {
            raw_artifact,
            protocol,
        })
        .await
    }

    pub async fn authorize_artifact(
        &self,
        encrypted_payload: Vec<u8>,
        ephemeral_public_key: [u8; 32],
    ) -> Result<Vec<u8>, String> {
        match self
            .send(KeypunkdRequest::AuthorizeArtifact {
                encrypted_payload,
                ephemeral_public_key,
            })
            .await?
        {
            KeypunkdResponse::ArtifactAuthorized { signed_artifact } => Ok(signed_artifact),
            KeypunkdResponse::Error { message } => Err(message),
            _ => Err("unexpected response variant".to_string()),
        }
    }

    pub async fn export_viewing_key(
        &self,
        protocol: ProtocolId,
        account: u32,
    ) -> Result<Vec<u8>, String> {
        match self
            .send(KeypunkdRequest::ExportViewingKey { protocol, account })
            .await?
        {
            KeypunkdResponse::ViewingKey { key } => Ok(key),
            KeypunkdResponse::Error { message } => Err(message),
            _ => Err("unexpected response variant".to_string()),
        }
    }
}
