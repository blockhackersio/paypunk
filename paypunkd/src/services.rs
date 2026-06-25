use paypunk_ipc::IpcMessage;
use paypunk_types::{Account, Balance, Intent, ProtocolId};
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

    pub async fn submit_intent(
        &self,
        intent: Intent,
        derivation_path: String,
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
        derivation_path: String,
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
        encrypted_password: Vec<u8>,
        client_public_key: [u8; 32],
        protocol: ProtocolId,
        derivation_path: String,
        index: u32,
    ) -> Result<String, String> {
        match self
            .send(PaypunkdRequest::DeriveAddress {
                encrypted_password,
                client_public_key,
                protocol,
                derivation_path,
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

    pub async fn broadcast_transaction(
        &self,
        protocol: ProtocolId,
        raw_tx: Vec<u8>,
    ) -> Result<String, String> {
        match self
            .send(PaypunkdRequest::BroadcastTransaction { protocol, raw_tx })
            .await?
        {
            PaypunkdResponse::TransactionBroadcasted { tx_hash } => Ok(tx_hash),
            PaypunkdResponse::Error { message } => Err(message),
            _ => Err("unexpected response variant".to_string()),
        }
    }

    pub async fn create_account(
        &self,
        protocol: ProtocolId,
        derivation_path: String,
        account_index: u32,
        name: String,
    ) -> Result<Account, String> {
        match self
            .send(PaypunkdRequest::CreateAccount {
                protocol,
                derivation_path,
                account_index,
                name,
            })
            .await?
        {
            PaypunkdResponse::AccountCreated { account } => Ok(account),
            PaypunkdResponse::Error { message } => Err(message),
            _ => Err("unexpected response variant".to_string()),
        }
    }

    pub async fn list_accounts(&self) -> Result<Vec<Account>, String> {
        match self.send(PaypunkdRequest::ListAccounts).await? {
            PaypunkdResponse::AccountsList { accounts } => Ok(accounts),
            PaypunkdResponse::Error { message } => Err(message),
            _ => Err("unexpected response variant".to_string()),
        }
    }

    pub async fn get_account(&self, id: String) -> Result<Option<Account>, String> {
        match self.send(PaypunkdRequest::GetAccount { id }).await? {
            PaypunkdResponse::AccountFound { account } => Ok(account),
            PaypunkdResponse::Error { message } => Err(message),
            _ => Err("unexpected response variant".to_string()),
        }
    }

    pub async fn get_paypunkd_encryption_key(&self) -> Result<[u8; 32], String> {
        match self.send(PaypunkdRequest::GetPaypunkdEncryptionKey).await? {
            PaypunkdResponse::PaypunkdEncryptionKey { key } => Ok(key),
            PaypunkdResponse::Error { message } => Err(message),
            _ => Err("unexpected response variant".to_string()),
        }
    }

    pub async fn has_seed(&self) -> Result<bool, String> {
        match self.send(PaypunkdRequest::HasSeed).await? {
            PaypunkdResponse::HasSeed { exists } => Ok(exists),
            PaypunkdResponse::Error { message } => Err(message),
            _ => Err("unexpected response variant".to_string()),
        }
    }

    pub async fn get_supported_protocols(&self) -> Result<Vec<ProtocolId>, String> {
        match self.send(PaypunkdRequest::GetSupportedProtocols).await? {
            PaypunkdResponse::SupportedProtocols { protocols } => Ok(protocols),
            PaypunkdResponse::Error { message } => Err(message),
            _ => Err("unexpected response variant".to_string()),
        }
    }

    pub async fn unlock(
        &self,
        encrypted_db_password: Vec<u8>,
        ephemeral_public_key: [u8; 32],
        encrypted_keypunkd_password: Vec<u8>,
        keypunkd_client_pk: [u8; 32],
        paths: Vec<(ProtocolId, String)>,
    ) -> Result<u32, String> {
        match self
            .send(PaypunkdRequest::Unlock {
                encrypted_db_password,
                ephemeral_public_key,
                encrypted_keypunkd_password,
                keypunkd_client_pk,
                paths,
            })
            .await?
        {
            PaypunkdResponse::UnlockSuccess { accounts_count } => Ok(accounts_count),
            PaypunkdResponse::Error { message } => Err(message),
            _ => Err("unexpected response variant".to_string()),
        }
    }

    pub async fn bulk_derive_accounts(
        &self,
        encrypted_password: Vec<u8>,
        client_public_key: [u8; 32],
        paths: Vec<(ProtocolId, String)>,
    ) -> Result<Vec<Account>, String> {
        match self
            .send(PaypunkdRequest::BulkDeriveAccounts {
                encrypted_password,
                client_public_key,
                paths,
            })
            .await?
        {
            PaypunkdResponse::AccountsBulkDerived { accounts } => Ok(accounts),
            PaypunkdResponse::Error { message } => Err(message),
            _ => Err("unexpected response variant".to_string()),
        }
    }
}
