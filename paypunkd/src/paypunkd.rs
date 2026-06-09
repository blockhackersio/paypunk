use std::collections::HashMap;

use paypunk_ipc::IpcMessage;
use paypunk_types::ProtocolId;
use tactix::{Actor, Ctx, Handler, Recipient};
use tracing::{debug, info, warn};

use crate::messages::{PaypunkdRequest, PaypunkdResponse};
use crate::usecases;

pub struct Paypunkd {
    keypunk_service: keypunkd::services::KeypunkService,
    protocols: HashMap<ProtocolId, Box<dyn paypunk_types::Protocol>>,
    /// Cache of public key bytes per (protocol, account).
    /// Populated lazily on first address derivation request for each protocol.
    public_keys: HashMap<(ProtocolId, u32), Vec<u8>>,
}

impl Paypunkd {
    pub fn new(
        recipient: Recipient<IpcMessage>,
        protocols: HashMap<ProtocolId, Box<dyn paypunk_types::Protocol>>,
    ) -> Self {
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

    async fn handle_derive_address(
        &mut self,
        protocol: ProtocolId,
        account: u32,
        index: u32,
    ) -> PaypunkdResponse {
        info!(?protocol, account, index, "handling DeriveAddress locally");
        let key = match self.get_or_fetch_public_key(protocol, account).await {
            Ok(k) => k.to_vec(),
            Err(e) => return PaypunkdResponse::Error { message: e },
        };
        let Some(protocol) = self.protocols.get(&protocol) else {
            let msg = format!("unknown protocol: {protocol:?}");
            warn!(error = %msg);
            return PaypunkdResponse::Error { message: msg };
        };
        match protocol.derive_address(&key, index) {
            Ok(address) => {
                debug!(%address, "address derived from cached public key");
                PaypunkdResponse::AddressDerived { address }
            }
            Err(e) => {
                warn!(error = %e, "address derivation from public key failed");
                PaypunkdResponse::Error { message: e }
            }
        }
    }
}

impl Actor for Paypunkd {}

impl Handler<IpcMessage> for Paypunkd {
    async fn handle(&mut self, msg: IpcMessage, _ctx: &Ctx<Self>) -> Result<Vec<u8>, String> {
        let request: PaypunkdRequest =
            postcard::from_bytes(&msg.payload).map_err(|e| format!("deserialize error: {e}"))?;

        debug!(?request, "dispatching request");

        let response = match request {
            PaypunkdRequest::GetKeypunkEncryptionKey => {
                info!("forwarding GetKeypunkEncryptionKey to keypunkd");
                match usecases::get_keypunk_encryption_key(&self.keypunk_service).await {
                    Ok(key) => PaypunkdResponse::KeypunkEncryptionKey { key },
                    Err(e) => {
                        warn!(error = %e, "GetKeypunkEncryptionKey failed");
                        PaypunkdResponse::Error { message: e }
                    }
                }
            }
            PaypunkdRequest::GenerateSeed {
                encrypted_password,
                client_public_key,
            } => {
                info!("forwarding GenerateSeed to keypunkd");
                match usecases::generate_seed(
                    &self.keypunk_service,
                    encrypted_password,
                    client_public_key,
                )
                .await
                {
                    Ok(encrypted_mnemonic) => {
                        PaypunkdResponse::SeedGenerated { encrypted_mnemonic }
                    }
                    Err(e) => {
                        warn!(error = %e, "GenerateSeed failed");
                        PaypunkdResponse::Error { message: e }
                    }
                }
            }
            PaypunkdRequest::RestoreSeed {
                encrypted_mnemonic,
                encrypted_password,
                client_public_key,
            } => {
                info!("forwarding RestoreSeed to keypunkd");
                match usecases::restore_seed(
                    &self.keypunk_service,
                    encrypted_mnemonic,
                    encrypted_password,
                    client_public_key,
                )
                .await
                {
                    Ok(()) => PaypunkdResponse::SeedRestored,
                    Err(e) => {
                        warn!(error = %e, "RestoreSeed failed");
                        PaypunkdResponse::Error { message: e }
                    }
                }
            }
            PaypunkdRequest::Unlock {
                encrypted_password,
                client_public_key,
            } => {
                info!("forwarding Unlock to keypunkd");
                match usecases::unlock(&self.keypunk_service, encrypted_password, client_public_key)
                    .await
                {
                    Ok(()) => PaypunkdResponse::Unlocked,
                    Err(e) => {
                        warn!(error = %e, "Unlock failed");
                        PaypunkdResponse::Error { message: e }
                    }
                }
            }
            PaypunkdRequest::DeriveAddress {
                protocol,
                account,
                index,
            } => Self::handle_derive_address(self, protocol, account, index).await,
            PaypunkdRequest::Sign {
                protocol,
                account,
                payload,
            } => {
                info!(?protocol, account, "forwarding Sign to keypunkd");
                match usecases::sign(&self.keypunk_service, protocol, account, payload).await {
                    Ok(signature) => PaypunkdResponse::Signature { signature },
                    Err(e) => {
                        warn!(error = %e, "Sign failed");
                        PaypunkdResponse::Error { message: e }
                    }
                }
            }
            PaypunkdRequest::Lock => {
                info!("forwarding Lock to keypunkd");
                match usecases::lock(&self.keypunk_service).await {
                    Ok(()) => PaypunkdResponse::Locked,
                    Err(e) => {
                        warn!(error = %e, "Lock failed");
                        PaypunkdResponse::Error { message: e }
                    }
                }
            }
        };

        let encoded =
            postcard::to_allocvec(&response).map_err(|e| format!("serialize error: {e}"))?;
        debug!(response_len = encoded.len(), "sending response");
        Ok(encoded)
    }
}
