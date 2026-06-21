use keypunkd::crypto::Keypair;
use paypunk_ipc::IpcMessage;
use paypunk_types::{caip, ProtocolId};
use tactix::{Actor, Ctx, Handler, Recipient};
use tracing::{debug, info, warn};

use crate::database::repository::SqliteAccountsRepository;
use crate::database::{AccountsRepository, Database};
use crate::messages::{PaypunkdRequest, PaypunkdResponse};
use crate::protocol_service::ProtocolService;
use crate::usecases;

pub struct Paypunkd {
    keypunk_service: keypunkd::services::KeypunkService,
    protocols: ProtocolService,
    db: Database,
    accounts_repo: Box<dyn AccountsRepository>,
    keystore: Keypair,
}

impl Paypunkd {
    pub fn new(
        recipient: Recipient<IpcMessage>,
        protocols: ProtocolService,
        db: Database,
        keystore: Keypair,
    ) -> Self {
        Self {
            keypunk_service: keypunkd::services::KeypunkService::new(recipient),
            protocols,
            db,
            accounts_repo: Box::new(SqliteAccountsRepository),
            keystore,
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

    async fn submit_intent(
        &self,
        intent: paypunk_types::Intent,
        derivation_path: Vec<u8>,
    ) -> PaypunkdResponse {
        info!("handling SubmitIntent");
        self.respond(
            "submit_intent",
            usecases::submit_intent(
                &self.keypunk_service,
                &self.protocols,
                &intent,
                &derivation_path,
            )
            .await,
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

    async fn get_balance(&self, address: String, asset: String) -> PaypunkdResponse {
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
            usecases::get_balance(&self.protocols, protocol, &address, &asset).await,
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
        let account_num = match caip::AccountId::parse(&account).and_then(|a| a.account_number()) {
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

    async fn broadcast_transaction(
        &self,
        protocol: ProtocolId,
        raw_tx: Vec<u8>,
    ) -> PaypunkdResponse {
        info!(?protocol, "broadcasting transaction");
        self.respond(
            "broadcast_transaction",
            usecases::broadcast_transaction(&self.protocols, protocol, &raw_tx).await,
            |tx_hash| PaypunkdResponse::TransactionBroadcasted { tx_hash },
        )
    }

    async fn create_account(
        &self,
        protocol: ProtocolId,
        derivation_path: String,
        account_index: u32,
        name: String,
    ) -> PaypunkdResponse {
        info!(?protocol, account_index, name, "creating account");
        self.respond(
            "create_account",
            usecases::create_account(
                &self.db,
                &self.protocols,
                self.accounts_repo.as_ref(),
                protocol,
                derivation_path,
                account_index,
                name,
            )
            .await,
            |account| PaypunkdResponse::AccountCreated { account },
        )
    }

    async fn list_accounts(&self) -> PaypunkdResponse {
        info!("listing accounts");
        self.respond(
            "list_accounts",
            usecases::list_accounts(&self.db, self.accounts_repo.as_ref()),
            |accounts| PaypunkdResponse::AccountsList { accounts },
        )
    }

    async fn get_account(&self, id: String) -> PaypunkdResponse {
        info!(id, "getting account");
        self.respond(
            "get_account",
            usecases::get_account(&self.db, self.accounts_repo.as_ref(), &id),
            |account| PaypunkdResponse::AccountFound { account },
        )
    }

    fn get_paypunkd_encryption_key(&self) -> PaypunkdResponse {
        info!("handling GetPaypunkdEncryptionKey");
        PaypunkdResponse::PaypunkdEncryptionKey {
            key: self.keystore.public_key(),
        }
    }

    async fn has_seed(&self) -> PaypunkdResponse {
        info!("forwarding HasSeed to keypunkd");
        self.respond(
            "has_seed",
            usecases::has_seed(&self.keypunk_service).await,
            |exists| PaypunkdResponse::HasSeed { exists },
        )
    }

    async fn unlock(
        &mut self,
        encrypted_db_password: Vec<u8>,
        ephemeral_public_key: [u8; 32],
        encrypted_keypunkd_password: Vec<u8>,
        keypunkd_client_pk: [u8; 32],
    ) -> PaypunkdResponse {
        info!("handling Unlock");

        // 1. If DB is already unlocked (fresh, no .enc file), skip decryption
        if self.db.is_locked() {
            // Decrypt the DB password using paypunkd's own keypair
            let decrypted_password = match self
                .keystore
                .decrypt(&encrypted_db_password, &ephemeral_public_key)
            {
                Ok(pw) => pw,
                Err(e) => {
                    return PaypunkdResponse::Error {
                        message: format!("failed to decrypt db password: {e}"),
                    }
                }
            };

            // 2. Unlock the database
            if let Err(e) = self.db.unlock(&decrypted_password) {
                return PaypunkdResponse::Error {
                    message: format!("failed to unlock database: {e}"),
                };
            }
        }

        // 3. Check if accounts exist
        let accounts = match usecases::list_accounts(&self.db, self.accounts_repo.as_ref()) {
            Ok(a) => a,
            Err(e) => {
                return PaypunkdResponse::Error {
                    message: format!("failed to list accounts: {e}"),
                }
            }
        };

        let accounts_count = accounts.len() as u32;

        // 4. If no accounts, bulk-derive from keypunkd and cache viewing keys
        if accounts.is_empty() {
            info!("no accounts found, bulk-deriving from keypunkd");
            let protocols = self.protocols.protocols();
            let keys = self
                .keypunk_service
                .bulk_export_viewing_keys(
                    encrypted_keypunkd_password,
                    keypunkd_client_pk,
                    protocols.clone(),
                    0,
                    30,
                )
                .await;

            match keys {
                Ok(derived) => {
                    // Store pre-derived keys in the database
                    for (protocol, account_index, viewing_key) in &derived {
                        let _ = usecases::save_pre_derived_key(
                            &self.db,
                            *protocol,
                            *account_index,
                            viewing_key,
                        );
                    }

                    // Create Ethereum Account 0 automatically
                    let _ = usecases::create_ethereum_account_0(
                        &self.db,
                        self.accounts_repo.as_ref(),
                        &self.protocols,
                    );

                    info!(count = derived.len(), "cached pre-derived viewing keys");
                    PaypunkdResponse::UnlockSuccess {
                        accounts_count: derived.len() as u32,
                    }
                }
                Err(e) => PaypunkdResponse::Error {
                    message: format!("failed to bulk-derive accounts: {e}"),
                },
            }
        } else {
            PaypunkdResponse::UnlockSuccess { accounts_count }
        }
    }

    async fn bulk_derive_accounts(
        &self,
        encrypted_password: Vec<u8>,
        client_public_key: [u8; 32],
        count: u32,
    ) -> PaypunkdResponse {
        info!("handling BulkDeriveAccounts");
        let protocols = self.protocols.protocols();
        self.respond(
            "bulk_derive_accounts",
            usecases::bulk_derive_accounts(
                &self.keypunk_service,
                &self.protocols,
                &self.db,
                self.accounts_repo.as_ref(),
                encrypted_password,
                client_public_key,
                protocols,
                count,
            )
            .await,
            |accounts| PaypunkdResponse::AccountsBulkDerived { accounts },
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
            PaypunkdRequest::SubmitIntent {
                intent,
                derivation_path,
            } => self.submit_intent(intent, derivation_path).await,
            PaypunkdRequest::ApproveSignature {
                encrypted_payload,
                ephemeral_public_key,
                derivation_path,
            } => {
                self.approve_signature(encrypted_payload, ephemeral_public_key, derivation_path)
                    .await
            }
            PaypunkdRequest::GetBalance { address, asset } => {
                self.get_balance(address, asset).await
            }
            PaypunkdRequest::DeriveAddress {
                encrypted_password,
                client_public_key,
                protocol,
                account,
                index,
            } => {
                self.derive_address(
                    encrypted_password,
                    client_public_key,
                    protocol,
                    account,
                    index,
                )
                .await
            }
            PaypunkdRequest::BroadcastTransaction { protocol, raw_tx } => {
                self.broadcast_transaction(protocol, raw_tx).await
            }
            PaypunkdRequest::CreateAccount {
                protocol,
                derivation_path,
                account_index,
                name,
            } => {
                self.create_account(protocol, derivation_path, account_index, name)
                    .await
            }
            PaypunkdRequest::ListAccounts => self.list_accounts().await,
            PaypunkdRequest::GetAccount { id } => self.get_account(id).await,
            PaypunkdRequest::GetPaypunkdEncryptionKey => self.get_paypunkd_encryption_key(),
            PaypunkdRequest::HasSeed => self.has_seed().await,
            PaypunkdRequest::Unlock {
                encrypted_db_password,
                ephemeral_public_key,
                encrypted_keypunkd_password,
                keypunkd_client_pk,
            } => {
                self.unlock(
                    encrypted_db_password,
                    ephemeral_public_key,
                    encrypted_keypunkd_password,
                    keypunkd_client_pk,
                )
                .await
            }
            PaypunkdRequest::BulkDeriveAccounts {
                encrypted_password,
                client_public_key,
                count,
            } => {
                self.bulk_derive_accounts(encrypted_password, client_public_key, count)
                    .await
            }
        };

        let encoded =
            postcard::to_allocvec(&response).map_err(|e| format!("serialize error: {e}"))?;
        debug!(response_len = encoded.len(), "sending response");
        Ok(encoded)
    }
}
