use paypunk_ipc::IpcMessage;
use tactix::{Actor, Ctx, Handler, Recipient};
use tracing::{debug, info, warn};

use crate::messages::{PaypunkdRequest, PaypunkdResponse};
use crate::usecases;

pub struct Paypunkd {
    keypunk_service: keypunkd::services::KeypunkService,
}

impl Paypunkd {
    pub fn new(recipient: Recipient<IpcMessage>) -> Self {
        Self {
            keypunk_service: keypunkd::services::KeypunkService::new(recipient),
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
            PaypunkdRequest::DeriveAddress { index } => {
                info!("forwarding DeriveAddress to keypunkd");
                match usecases::derive_address(&self.keypunk_service, index).await {
                    Ok(address) => PaypunkdResponse::AddressDerived { address },
                    Err(e) => {
                        warn!(error = %e, "DeriveAddress failed");
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
