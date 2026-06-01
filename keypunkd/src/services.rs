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

    pub async fn get_public_key(&self) -> Result<[u8; 32], String> {
        let request = KeypunkdRequest::GetPublicKey;
        let payload =
            postcard::to_allocvec(&request).map_err(|e| format!("serialize error: {e}"))?;
        let msg = IpcMessage::new(payload);
        let response_bytes = self.recipient.ask(msg).await?;
        let response: KeypunkdResponse =
            postcard::from_bytes(&response_bytes).map_err(|e| format!("deserialize error: {e}"))?;
        match response {
            KeypunkdResponse::PublicKey { key } => Ok(key),
            KeypunkdResponse::Error { message } => Err(message),
            _ => Err("unexpected response variant".to_string()),
        }
    }

    pub async fn generate_seed(
        &self,
        encrypted_password: Vec<u8>,
        client_public_key: [u8; 32],
    ) -> Result<Vec<u8>, String> {
        let request = KeypunkdRequest::GenerateSeed {
            encrypted_password,
            client_public_key,
        };
        let payload =
            postcard::to_allocvec(&request).map_err(|e| format!("serialize error: {e}"))?;
        let msg = IpcMessage::new(payload);
        let response_bytes = self.recipient.ask(msg).await?;
        let response: KeypunkdResponse =
            postcard::from_bytes(&response_bytes).map_err(|e| format!("deserialize error: {e}"))?;
        match response {
            KeypunkdResponse::SeedGenerated { encrypted_mnemonic } => Ok(encrypted_mnemonic),
            KeypunkdResponse::Error { message } => Err(message),
            _ => Err("unexpected response variant".to_string()),
        }
    }
}
