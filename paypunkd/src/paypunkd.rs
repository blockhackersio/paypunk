use paypunk_ipc::IpcMessage;
use paypunk_types::{caip, ProtocolId};
use tactix::{Actor, Ctx, Handler, Recipient};
use tracing::{debug, info, warn};

use crate::messages::{PaypunkdRequest, PaypunkdResponse};
use crate::protocol_service::ProtocolService;
use crate::usecases;

pub struct Paypunkd {
    keypunk_service: keypunkd::services::KeypunkService,
    protocols: ProtocolService,
}

impl Paypunkd {
    pub fn new(recipient: Recipient<IpcMessage>, protocols: ProtocolService) -> Self {
        Self {
            keypunk_service: keypunkd::services::KeypunkService::new(recipient),
            protocols,
        }
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

    async fn submit_intent(&self, intent: paypunk_types::Intent, derivation_path: Vec<u8>) -> PaypunkdResponse {
        info!("handling SubmitIntent");
        self.respond(
            "submit_intent",
            usecases::submit_intent(&self.keypunk_service, &self.protocols, &intent, &derivation_path).await,
            |(raw_artifact, parsed_summary, keypunkd_signature, keypunkd_public_key)| {
                PaypunkdResponse::SignablePreview {
                    raw_artifact,
                    parsed_summary,
                    keypunkd_signature,
                    keypunkd_public_key,
                }
            },
        )
    }

    async fn approve_signature(
        &self,
        encrypted_payload: Vec<u8>,
        ephemeral_public_key: [u8; 32],
        derivation_path: Vec<u8>,
    ) -> PaypunkdResponse {
        info!("handling ApproveSignature");
        self.respond(
            "approve_signature",
            usecases::approve_signature(
                &self.keypunk_service,
                encrypted_payload,
                ephemeral_public_key,
                derivation_path,
            )
            .await,
            |signed_artifact| PaypunkdResponse::SignatureApproved { signed_artifact },
        )
    }

    async fn get_balance(
        &self,
        address: String,
        asset: String,
    ) -> PaypunkdResponse {
        info!("querying balance");
        let protocol = match address.split(':').next().unwrap_or("") {
            "zcash" => ProtocolId::Zcash,
            "eip155" => ProtocolId::Ethereum,
            _ => {
                return PaypunkdResponse::Error {
                    message: format!("unknown chain in address: {address}"),
                }
            }
        };
        self.respond(
            "get_balance",
            usecases::get_balance(&self.protocols, protocol, &address, &asset),
            |balance| PaypunkdResponse::Balance { balance },
        )
    }

    async fn derive_address(
        &mut self,
        encrypted_password: Vec<u8>,
        client_public_key: [u8; 32],
        protocol: ProtocolId,
        account: String,
        index: u32,
    ) -> PaypunkdResponse {
        info!(?protocol, account, index, "deriving address");
        let account_num = match caip::AccountId::parse(&account)
            .and_then(|a| a.account_number())
        {
            Ok(n) => n,
            Err(e) => {
                return PaypunkdResponse::Error {
                    message: format!("invalid CAIP-10 account: {e}"),
                }
            }
        };
        self.respond(
            "derive_address",
            usecases::export_viewing_key(
                &self.keypunk_service,
                encrypted_password,
                client_public_key,
                protocol,
                account_num,
            )
            .await
            .and_then(|viewing_key| {
                usecases::derive_address(&self.protocols, protocol, &viewing_key, index)
            }),
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
            PaypunkdRequest::SubmitIntent { intent, derivation_path } => self.submit_intent(intent, derivation_path).await,
            PaypunkdRequest::ApproveSignature {
                encrypted_payload,
                ephemeral_public_key,
                derivation_path,
            } => self.approve_signature(encrypted_payload, ephemeral_public_key, derivation_path).await,
            PaypunkdRequest::GetBalance { address, asset } => {
                self.get_balance(address, asset).await
            }
            PaypunkdRequest::DeriveAddress {
                encrypted_password,
                client_public_key,
                protocol,
                account,
                index,
            } => self.derive_address(encrypted_password, client_public_key, protocol, account, index).await,
        };

        let encoded =
            postcard::to_allocvec(&response).map_err(|e| format!("serialize error: {e}"))?;
        debug!(response_len = encoded.len(), "sending response");
        Ok(encoded)
    }
}
