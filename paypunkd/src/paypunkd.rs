use std::collections::HashMap;

use paypunk_ipc::IpcMessage;
use paypunk_types::ProtocolId;
use tactix::{Actor, Ctx, Handler, Recipient};
use tracing::{debug, info, warn};

use crate::messages::{PaypunkdRequest, PaypunkdResponse};
use crate::usecases;

pub struct Paypunkd {
    keypunk_service: keypunkd::services::KeypunkService,
    /// Cache of view key bytes per (protocol, account).
    /// Populated lazily on first address derivation request for each protocol.
    view_keys: HashMap<(ProtocolId, u32), Vec<u8>>,
}

impl Paypunkd {
    pub fn new(recipient: Recipient<IpcMessage>) -> Self {
        Self {
            keypunk_service: keypunkd::services::KeypunkService::new(recipient),
            view_keys: HashMap::new(),
        }
    }

    async fn get_or_fetch_view_key(
        &mut self,
        protocol: ProtocolId,
        account: u32,
    ) -> Result<&[u8], String> {
        if !self.view_keys.contains_key(&(protocol, account)) {
            debug!(?protocol, account, "fetching view key from keypunkd");
            let key = usecases::derive_view_key(&self.keypunk_service, protocol, account).await?;
            self.view_keys.insert((protocol, account), key);
        }
        Ok(self.view_keys.get(&(protocol, account)).unwrap())
    }
}

impl Actor for Paypunkd {}

impl Handler<IpcMessage> for Paypunkd {
    async fn handle(&mut self, msg: IpcMessage, _ctx: &Ctx<Self>) -> Result<Vec<u8>, String> {
        let request: PaypunkdRequest =
            postcard::from_bytes(&msg.payload).map_err(|e| format!("deserialize error: {e}"))?;

        debug!(?request, "dispatching request");

        let response = match request {
            PaypunkdRequest::GetKeypunkPublicKey => {
                info!("forwarding GetKeypunkPublicKey to keypunkd");
                match usecases::get_keypunk_public_key(&self.keypunk_service).await {
                    Ok(key) => PaypunkdResponse::KeypunkPublicKey { key },
                    Err(e) => {
                        warn!(error = %e, "GetKeypunkPublicKey failed");
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
                match usecases::unlock(
                    &self.keypunk_service,
                    encrypted_password,
                    client_public_key,
                )
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
            } => {
                info!(?protocol, account, index, "handling DeriveAddress locally");
                match self.get_or_fetch_view_key(protocol, account).await {
                    Ok(fvk_bytes) => {
                        match paypunk_chains_zcash::address::derive_from_fvk(fvk_bytes, index) {
                            Ok(address) => {
                                debug!(%address, "address derived from cached view key");
                                PaypunkdResponse::AddressDerived { address }
                            }
                            Err(e) => {
                                warn!(error = %e, "address derivation from view key failed");
                                PaypunkdResponse::Error {
                                    message: e.to_string(),
                                }
                            }
                        }
                    }
                    Err(e) => PaypunkdResponse::Error { message: e },
                }
            }
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
