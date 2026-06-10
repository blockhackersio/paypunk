use std::collections::HashMap;

use paypunk_ipc::IpcMessage;
use paypunk_types::ProtocolId;
use tactix::{Actor, Ctx, Handler, Recipient};
use tracing::{debug, info, warn};

use crate::messages::{PaypunkdRequest, PaypunkdResponse};
use crate::protocol_service::ProtocolService;
use crate::usecases;

pub struct Paypunkd {
    keypunk_service: keypunkd::services::KeypunkService,
    protocols: ProtocolService,
    public_keys: HashMap<(ProtocolId, u32), Vec<u8>>,
}

impl Paypunkd {
    pub fn new(recipient: Recipient<IpcMessage>, protocols: ProtocolService) -> Self {
        Self {
            keypunk_service: keypunkd::services::KeypunkService::new(recipient),
            protocols,
            public_keys: HashMap::new(),
        }
    }

    async fn get_or_fetch_public_key(
        &mut self,
        protocol: ProtocolId,
        account: u32,
    ) -> Result<&[u8], String> {
        if !self.public_keys.contains_key(&(protocol, account)) {
            debug!(?protocol, account, "fetching public key from keypunkd");
            let key = usecases::derive_public_key(&self.keypunk_service, protocol, account).await?;
            self.public_keys.insert((protocol, account), key);
        }
        Ok(self.public_keys.get(&(protocol, account)).unwrap())
    }

    fn respond<T>(
        &self,
        label: &str,
        result: Result<T, String>,
        map_ok: impl FnOnce(T) -> PaypunkdResponse,
    ) -> PaypunkdResponse {
        match result {
            Ok(v) => map_ok(v),
            Err(e) => {
                warn!(error = %e, "{label} failed");
                PaypunkdResponse::Error { message: e }
            }
        }
    }

    async fn get_keypunk_encryption_key(&self) -> PaypunkdResponse {
        info!("forwarding GetKeypunkEncryptionKey to keypunkd");
        self.respond(
            "get_keypunk_encryption_key",
            usecases::get_keypunk_encryption_key(&self.keypunk_service).await,
            |key| PaypunkdResponse::KeypunkEncryptionKey { key },
        )
    }

    async fn generate_seed(
        &self,
        encrypted_password: Vec<u8>,
        client_public_key: [u8; 32],
    ) -> PaypunkdResponse {
        info!("forwarding GenerateSeed to keypunkd");
        self.respond(
            "generate_seed",
            usecases::generate_seed(&self.keypunk_service, encrypted_password, client_public_key)
                .await,
            |encrypted_mnemonic| PaypunkdResponse::SeedGenerated { encrypted_mnemonic },
        )
    }

    async fn restore_seed(
        &self,
        encrypted_mnemonic: Vec<u8>,
        encrypted_password: Vec<u8>,
        client_public_key: [u8; 32],
    ) -> PaypunkdResponse {
        info!("forwarding RestoreSeed to keypunkd");
        self.respond(
            "restore_seed",
            usecases::restore_seed(
                &self.keypunk_service,
                encrypted_mnemonic,
                encrypted_password,
                client_public_key,
            )
            .await,
            |()| PaypunkdResponse::SeedRestored,
        )
    }

    async fn unlock(
        &self,
        encrypted_password: Vec<u8>,
        client_public_key: [u8; 32],
    ) -> PaypunkdResponse {
        info!("forwarding Unlock to keypunkd");
        self.respond(
            "unlock",
            usecases::unlock(&self.keypunk_service, encrypted_password, client_public_key).await,
            |()| PaypunkdResponse::Unlocked,
        )
    }

    async fn sign(&self, protocol: ProtocolId, account: u32, payload: Vec<u8>) -> PaypunkdResponse {
        info!(?protocol, account, "forwarding Sign to keypunkd");
        self.respond(
            "sign",
            usecases::sign(&self.keypunk_service, protocol, account, payload).await,
            |signature| PaypunkdResponse::Signature { signature },
        )
    }

    async fn lock(&self) -> PaypunkdResponse {
        info!("forwarding Lock to keypunkd");
        self.respond("lock", usecases::lock(&self.keypunk_service).await, |()| {
            PaypunkdResponse::Locked
        })
    }

    async fn derive_address(
        &mut self,
        protocol: ProtocolId,
        account: u32,
        index: u32,
    ) -> PaypunkdResponse {
        info!(?protocol, account, index, "deriving address");
        let key = match self.get_or_fetch_public_key(protocol, account).await {
            Ok(k) => k.to_vec(),
            Err(e) => {
                warn!(error = %e, "DeriveAddress failed");
                return PaypunkdResponse::Error { message: e };
            }
        };
        self.respond(
            "derive_address",
            usecases::derive_address(&self.protocols, protocol, &key, index),
            |address| PaypunkdResponse::AddressDerived { address },
        )
    }
}

impl Actor for Paypunkd {}

impl Handler<IpcMessage> for Paypunkd {
    async fn handle(&mut self, msg: IpcMessage, _ctx: &Ctx<Self>) -> Result<Vec<u8>, String> {
        let request: PaypunkdRequest =
            postcard::from_bytes(&msg.payload).map_err(|e| format!("deserialize error: {e}"))?;

        debug!(?request, "dispatching request");

        let response = match request {
            PaypunkdRequest::GetKeypunkEncryptionKey => self.get_keypunk_encryption_key().await,
            PaypunkdRequest::GenerateSeed {
                encrypted_password,
                client_public_key,
            } => {
                self.generate_seed(encrypted_password, client_public_key)
                    .await
            }
            PaypunkdRequest::RestoreSeed {
                encrypted_mnemonic,
                encrypted_password,
                client_public_key,
            } => {
                self.restore_seed(encrypted_mnemonic, encrypted_password, client_public_key)
                    .await
            }
            PaypunkdRequest::Unlock {
                encrypted_password,
                client_public_key,
            } => self.unlock(encrypted_password, client_public_key).await,
            PaypunkdRequest::DeriveAddress {
                protocol,
                account,
                index,
            } => self.derive_address(protocol, account, index).await,
            PaypunkdRequest::Sign {
                protocol,
                account,
                payload,
            } => self.sign(protocol, account, payload).await,
            PaypunkdRequest::Lock => self.lock().await,
        };

        let encoded =
            postcard::to_allocvec(&response).map_err(|e| format!("serialize error: {e}"))?;
        debug!(response_len = encoded.len(), "sending response");
        Ok(encoded)
    }
}
