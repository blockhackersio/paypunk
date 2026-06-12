use paypunk_ipc::IpcMessage;
use paypunk_types::{Balance, Intent, ProtocolId};
use tactix::{Recipient, Sender};

use crate::messages::{PaypunkdRequest, PaypunkdResponse};

pub struct PaypunkService {
    recipient: Recipient<IpcMessage>,
}

impl PaypunkService {
    pub fn new(recipient: Recipient<IpcMessage>) -> Self {
        Self { recipient }
    }

    async fn send(&self, request: PaypunkdRequest) -> Result<PaypunkdResponse, String> {
        let payload =
            postcard::to_allocvec(&request).map_err(|e| format!("serialize error: {e}"))?;
        let msg = IpcMessage::new(payload);
        let response_bytes = self.recipient.ask(msg).await?;
        postcard::from_bytes(&response_bytes).map_err(|e| format!("deserialize error: {e}"))
    }

    pub async fn get_keypunk_encryption_key(&self) -> Result<[u8; 32], String> {
        match self.send(PaypunkdRequest::GetKeypunkEncryptionKey).await? {
            PaypunkdResponse::KeypunkEncryptionKey { key } => Ok(key),
            PaypunkdResponse::Error { message } => Err(message),
            _ => Err("unexpected response variant".to_string()),
        }
    }

    pub async fn generate_seed(
        &self,
        encrypted_password: Vec<u8>,
        client_public_key: [u8; 32],
    ) -> Result<Vec<u8>, String> {
        match self
            .send(PaypunkdRequest::GenerateSeed {
                encrypted_password,
                client_public_key,
            })
            .await?
        {
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
        match self
            .send(PaypunkdRequest::RestoreSeed {
                encrypted_mnemonic,
                encrypted_password,
                client_public_key,
            })
            .await?
        {
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
        match self
            .send(PaypunkdRequest::Unlock {
                encrypted_password,
                client_public_key,
            })
            .await?
        {
            PaypunkdResponse::Unlocked => Ok(()),
            PaypunkdResponse::Error { message } => Err(message),
            _ => Err("unexpected response variant".to_string()),
        }
    }

    pub async fn lock(&self) -> Result<(), String> {
        match self.send(PaypunkdRequest::Lock).await? {
            PaypunkdResponse::Locked => Ok(()),
            PaypunkdResponse::Error { message } => Err(message),
            _ => Err("unexpected response variant".to_string()),
        }
    }

    pub async fn submit_intent(
        &self,
        intent: Intent,
        derivation_path: Vec<u8>,
    ) -> Result<PaypunkdResponse, String> {
        self.send(PaypunkdRequest::SubmitIntent {
            intent,
            derivation_path,
        })
        .await
    }

    pub async fn approve_signature(
        &self,
        encrypted_payload: Vec<u8>,
        ephemeral_public_key: [u8; 32],
        derivation_path: Vec<u8>,
    ) -> Result<Vec<u8>, String> {
        match self
            .send(PaypunkdRequest::ApproveSignature {
                encrypted_payload,
                ephemeral_public_key,
                derivation_path,
            })
            .await?
        {
            PaypunkdResponse::SignatureApproved { signed_artifact } => Ok(signed_artifact),
            PaypunkdResponse::Error { message } => Err(message),
            _ => Err("unexpected response variant".to_string()),
        }
    }

    pub async fn derive_address(
        &self,
        protocol: ProtocolId,
        account: String,
        index: u32,
    ) -> Result<String, String> {
        match self
            .send(PaypunkdRequest::DeriveAddress {
                protocol,
                account,
                index,
            })
            .await?
        {
            PaypunkdResponse::AddressDerived { address } => Ok(address),
            PaypunkdResponse::Error { message } => Err(message),
            _ => Err("unexpected response variant".to_string()),
        }
    }

    pub async fn get_balance(&self, address: String, asset: String) -> Result<Balance, String> {
        match self
            .send(PaypunkdRequest::GetBalance { address, asset })
            .await?
        {
            PaypunkdResponse::Balance { balance } => Ok(balance),
            PaypunkdResponse::Error { message } => Err(message),
            _ => Err("unexpected response variant".to_string()),
        }
    }
}
