use paypunk_ipc::IpcMessage;
use paypunk_ipc::IpcSender;
use paypunk_types::{Account, AssetId, Balance, Intent, ProtocolId};
use paypunkd::services::PaypunkService;
use tactix::{Recipient, Sender};
use zeroize::Zeroizing;

/// High-level wallet client that hides IPC and service details.
pub struct Client {
    service: PaypunkService,
}

impl Client {
    /// Connect to a running `paypunkd` instance over its Unix socket.
    pub async fn connect(socket_path: &str) -> Result<Self, String> {
        let ipc = IpcSender::connect(socket_path)
            .await
            .map_err(|e| e.to_string())?;
        let service = PaypunkService::new(ipc.recipient());
        Ok(Self { service })
    }

    /// Create a client from an existing IPC recipient, bypassing Unix sockets.
    /// Useful for testing where actors are wired directly in-process.
    pub fn with_recipient(recipient: Recipient<IpcMessage>) -> Self {
        Self {
            service: PaypunkService::new(recipient),
        }
    }

    /// Generate a new wallet seed, encrypt it with the given password,
    /// and return the 12-word BIP39 mnemonic.
    pub async fn generate_seed(
        &self,
        password: Zeroizing<String>,
    ) -> Result<Zeroizing<String>, String> {
        crate::functions::generate_seed(&self.service, password).await
    }

    /// Restore a wallet from an existing BIP39 mnemonic seed phrase and password.
    pub async fn restore_seed(
        &self,
        mnemonic: Zeroizing<String>,
        password: Zeroizing<String>,
    ) -> Result<(), String> {
        crate::functions::restore_seed(&self.service, mnemonic, password).await
    }

    /// Derive an address for the given protocol, CAIP-10 account, and diversifier index.
    ///
    /// Fetches the viewing key from keypunkd (using the wallet password) and derives
    /// the address locally via the protocol implementation.
    pub async fn derive_address(
        &self,
        password: Zeroizing<String>,
        protocol: ProtocolId,
        account: String,
        index: u32,
    ) -> Result<String, String> {
        crate::functions::derive_address(&self.service, password, protocol, account, index).await
    }

    /// Submit an intent for the two-phase authorization flow.
    ///
    /// Phase 1: Builds the unsigned artifact, sends it to keypunkd for
    /// parsing and preview, and returns the preview data for user approval.
    pub async fn submit_intent(
        &self,
        intent: Intent,
        derivation_path: &[u8],
    ) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>, [u8; 32]), String> {
        crate::functions::submit_intent(&self.service, intent, derivation_path).await
    }

    /// Approve a previously previewed artifact.
    ///
    /// Phase 2: Encrypts the password along with the artifact and keypunkd's
    /// signature to keypunkd's public key, then sends for authorization.
    pub async fn approve_signature(
        &self,
        raw_artifact: &[u8],
        keypunkd_signature: &[u8],
        password: Zeroizing<String>,
        derivation_path: &[u8],
    ) -> Result<Vec<u8>, String> {
        crate::functions::approve_signature(
            &self.service,
            raw_artifact,
            keypunkd_signature,
            password,
            derivation_path,
        )
        .await
    }

    /// Query the balance for the given address and asset (CAIP-10 and CAIP-19).
    pub async fn get_balance(
        &self,
        address: String,
        asset: String,
    ) -> Result<Balance, String> {
        crate::functions::get_balance(&self.service, address, asset).await
    }

    /// Legacy balance query using protocol + account + AssetId.
    pub async fn get_balance_legacy(
        &self,
        protocol: ProtocolId,
        account: u32,
        asset: AssetId,
    ) -> Result<Balance, String> {
        crate::functions::get_balance_legacy(&self.service, protocol, account, asset).await
    }

    /// Broadcast a finalized, signed transaction to the network.
    pub async fn broadcast_transaction(
        &self,
        protocol: ProtocolId,
        raw_tx: Vec<u8>,
    ) -> Result<String, String> {
        crate::functions::broadcast_transaction(&self.service, protocol, raw_tx).await
    }

    /// Create a new account from a pre-derived viewing key (no password needed).
    /// Accounts must be pre-derived via unlock (indices 0-29).
    pub async fn create_account(
        &self,
        protocol: ProtocolId,
        derivation_path: String,
        account_index: u32,
        name: String,
    ) -> Result<Account, String> {
        crate::functions::create_account(
            &self.service,
            protocol,
            derivation_path,
            account_index,
            name,
        )
        .await
    }

    /// List all accounts from the database.
    pub async fn list_accounts(&self) -> Result<Vec<Account>, String> {
        crate::functions::list_accounts(&self.service).await
    }

    /// Get a single account by ID.
    pub async fn get_account(&self, id: String) -> Result<Option<Account>, String> {
        crate::functions::get_account(&self.service, id).await
    }

    /// Check whether a wallet seed exists on keypunkd.
    pub async fn check_wallet_exists(&self) -> Result<bool, String> {
        crate::functions::check_wallet_exists(&self.service).await
    }

    /// Unlock the wallet by decrypting the DB and deriving initial accounts.
    pub async fn unlock(
        &self,
        password: Zeroizing<String>,
    ) -> Result<u32, String> {
        crate::functions::unlock(&self.service, password).await
    }

    /// Get paypunkd's public encryption key.
    pub async fn get_paypunkd_encryption_key(&self) -> Result<[u8; 32], String> {
        self.service.get_paypunkd_encryption_key().await
    }
}
