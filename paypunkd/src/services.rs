use paypunk_ipc::IpcMessage;
use tactix::{Recipient, Sender};

pub struct PaypunkService {
    recipient: Recipient<IpcMessage>,
}

impl PaypunkService {
    pub fn new(recipient: Recipient<IpcMessage>) -> Self {
        Self { recipient }
    }

    pub async fn get_keypunk_public_key(&self) -> Result<[u8; 32], String> {
        let request = crate::messages::PaypunkdRequest::GetKeypunkPublicKey;
        let payload =
            postcard::to_allocvec(&request).map_err(|e| format!("serialize error: {e}"))?;
        let msg = IpcMessage {
            payload,
            sender_public_key: None,
        };
        let response_bytes = self.recipient.ask(msg).await?;
        let response: crate::messages::PaypunkdResponse =
            postcard::from_bytes(&response_bytes).map_err(|e| format!("deserialize error: {e}"))?;
        match response {
            crate::messages::PaypunkdResponse::KeypunkPublicKey { key } => Ok(key),
            crate::messages::PaypunkdResponse::Error { message } => Err(message),
            _ => Err("unexpected response variant".to_string()),
        }
    }

    pub async fn generate_seed(
        &self,
        encrypted_password: Vec<u8>,
        client_public_key: [u8; 32],
    ) -> Result<Vec<u8>, String> {
        let request = crate::messages::PaypunkdRequest::GenerateSeed {
            encrypted_password,
            client_public_key,
        };
        let payload =
            postcard::to_allocvec(&request).map_err(|e| format!("serialize error: {e}"))?;
        let msg = IpcMessage {
            payload,
            sender_public_key: None,
        };
        let response_bytes = self.recipient.ask(msg).await?;
        let response: crate::messages::PaypunkdResponse =
            postcard::from_bytes(&response_bytes).map_err(|e| format!("deserialize error: {e}"))?;
        match response {
            crate::messages::PaypunkdResponse::SeedGenerated { encrypted_mnemonic } => {
                Ok(encrypted_mnemonic)
            }
            crate::messages::PaypunkdResponse::Error { message } => Err(message),
            _ => Err("unexpected response variant".to_string()),
        }
    }
}
