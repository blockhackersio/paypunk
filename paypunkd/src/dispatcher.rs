use paypunk_ipc::IpcMessage;
use tactix::{Actor, Ctx, Handler, Recipient};

use crate::messages::{PaypunkdRequest, PaypunkdResponse};
use crate::usecases;

pub struct Dispatcher {
    keypunk_service: keypunkd::services::KeypunkService,
}

impl Dispatcher {
    pub fn new(recipient: Recipient<IpcMessage>) -> Self {
        Self {
            keypunk_service: keypunkd::services::KeypunkService::new(recipient),
        }
    }
}

impl Actor for Dispatcher {}

impl Handler<IpcMessage> for Dispatcher {
    async fn handle(&mut self, msg: IpcMessage, _ctx: &Ctx<Self>) -> Result<Vec<u8>, String> {
        let request: PaypunkdRequest =
            postcard::from_bytes(&msg.payload).map_err(|e| format!("deserialize error: {e}"))?;

        let response = match request {
            PaypunkdRequest::GetKeypunkPublicKey => {
                match usecases::get_keypunk_public_key(&self.keypunk_service).await {
                    Ok(key) => PaypunkdResponse::KeypunkPublicKey { key },
                    Err(e) => PaypunkdResponse::Error { message: e },
                }
            }
            PaypunkdRequest::GenerateSeed {
                encrypted_password,
                client_public_key,
            } => {
                match usecases::generate_seed(&self.keypunk_service, encrypted_password, client_public_key).await {
                    Ok(encrypted_mnemonic) => PaypunkdResponse::SeedGenerated { encrypted_mnemonic },
                    Err(e) => PaypunkdResponse::Error { message: e },
                }
            }
        };

        postcard::to_allocvec(&response).map_err(|e| format!("serialize error: {e}"))
    }
}
