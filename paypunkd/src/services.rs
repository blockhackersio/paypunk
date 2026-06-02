use paypunk_ipc::IpcMessage;
use tactix::{Recipient, Sender};

use crate::messages::{PaypunkdRequest, PaypunkdResponse};

pub struct PaypunkService {
    recipient: Recipient<IpcMessage>,
}

impl PaypunkService {
    pub fn new(recipient: Recipient<IpcMessage>) -> Self {
        Self { recipient }
    }

    pub async fn get_keypunk_public_key(&self) -> Result<[u8; 32], String> {
        let request = PaypunkdRequest::GetKeypunkPublicKey;
        let payload =
            postcard::to_allocvec(&request).map_err(|e| format!("serialize error: {e}"))?;
        let msg = IpcMessage::new(payload);
        let response_bytes = self.recipient.ask(msg).await?;
        let response: PaypunkdResponse =
            postcard::from_bytes(&response_bytes).map_err(|e| format!("deserialize error: {e}"))?;
        match response {
            PaypunkdResponse::KeypunkPublicKey { key } => Ok(key),
            PaypunkdResponse::Error { message } => Err(message),
            _ => Err("unexpected response variant".to_string()),
        }
    }

    pub async fn generate_seed(
        &self,
        encrypted_password: Vec<u8>,
        client_public_key: [u8; 32],
    ) -> Result<Vec<u8>, String> {
        let request = PaypunkdRequest::GenerateSeed {
            encrypted_password,
            client_public_key,
        };
        let payload =
            postcard::to_allocvec(&request).map_err(|e| format!("serialize error: {e}"))?;
        let msg = IpcMessage::new(payload);
        let response_bytes = self.recipient.ask(msg).await?;
        let response: PaypunkdResponse =
            postcard::from_bytes(&response_bytes).map_err(|e| format!("deserialize error: {e}"))?;
        match response {
            PaypunkdResponse::SeedGenerated { encrypted_mnemonic } => Ok(encrypted_mnemonic),
            PaypunkdResponse::Error { message } => Err(message),
            _ => Err("unexpected response variant".to_string()),
        }
    }

    pub async fn restore_seed(
        &self,
        encrypted_mnemonic: Vec<u8>,
        encrypted_password: Vec<u8>,
        client_public_key: [u8; 32],
    ) -> Result<(), String> {
        let request = PaypunkdRequest::RestoreSeed {
            encrypted_mnemonic,
            encrypted_password,
            client_public_key,
        };
        let payload =
            postcard::to_allocvec(&request).map_err(|e| format!("serialize error: {e}"))?;
        let msg = IpcMessage::new(payload);
        let response_bytes = self.recipient.ask(msg).await?;
        let response: PaypunkdResponse =
            postcard::from_bytes(&response_bytes).map_err(|e| format!("deserialize error: {e}"))?;
        match response {
            PaypunkdResponse::SeedRestored => Ok(()),
            PaypunkdResponse::Error { message } => Err(message),
            _ => Err("unexpected response variant".to_string()),
        }
    }

    pub async fn unlock(
        &self,
        encrypted_password: Vec<u8>,
        client_public_key: [u8; 32],
    ) -> Result<(), String> {
        let request = PaypunkdRequest::Unlock {
            encrypted_password,
            client_public_key,
        };
        let payload =
            postcard::to_allocvec(&request).map_err(|e| format!("serialize error: {e}"))?;
        let msg = IpcMessage::new(payload);
        let response_bytes = self.recipient.ask(msg).await?;
        let response: PaypunkdResponse =
            postcard::from_bytes(&response_bytes).map_err(|e| format!("deserialize error: {e}"))?;
        match response {
            PaypunkdResponse::Unlocked => Ok(()),
            PaypunkdResponse::Error { message } => Err(message),
            _ => Err("unexpected response variant".to_string()),
        }
    }

    pub async fn derive_address(&self, index: u32) -> Result<String, String> {
        let request = PaypunkdRequest::DeriveAddress { index };
        let payload =
            postcard::to_allocvec(&request).map_err(|e| format!("serialize error: {e}"))?;
        let msg = IpcMessage::new(payload);
        let response_bytes = self.recipient.ask(msg).await?;
        let response: PaypunkdResponse =
            postcard::from_bytes(&response_bytes).map_err(|e| format!("deserialize error: {e}"))?;
        match response {
            PaypunkdResponse::AddressDerived { address } => Ok(address),
            PaypunkdResponse::Error { message } => Err(message),
            _ => Err("unexpected response variant".to_string()),
        }
    }

    pub async fn lock(&self) -> Result<(), String> {
        let request = PaypunkdRequest::Lock;
        let payload =
            postcard::to_allocvec(&request).map_err(|e| format!("serialize error: {e}"))?;
        let msg = IpcMessage::new(payload);
        let response_bytes = self.recipient.ask(msg).await?;
        let response: PaypunkdResponse =
            postcard::from_bytes(&response_bytes).map_err(|e| format!("deserialize error: {e}"))?;
        match response {
            PaypunkdResponse::Locked => Ok(()),
            PaypunkdResponse::Error { message } => Err(message),
            _ => Err("unexpected response variant".to_string()),
        }
    }
}
