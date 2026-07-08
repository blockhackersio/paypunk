use serde::{Deserialize, Serialize};

pub mod caip;
pub use caip::ChainId;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ProtocolId {
    Zcash,
    Bitcoin,
    Ethereum,
    Monero,
    Solana,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum AssetId {
    Native,
    Token(String),
}

// ── Intent types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Intent {
    Zcash(ZcashIntent),
    Ethereum(EthereumIntent),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ZcashIntent {
    Transfer {
        to: String,
        amount: String,
        from: String,
        asset: String,
        memo: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EthereumIntent {
    Transfer {
        to: String,
        amount: String,
        from: String,
        asset: String,
        data: Option<String>,
    },
    ContractCall {
        to: String,
        amount: String,
        from: String,
        asset: String,
        data: String,
    },
}

// ── Artifact summary ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArtifactSummary {
    pub to: String,
    pub amount: String,
    pub fee: String,
    pub nonce: u64,
    pub memo: Option<String>,
    pub protocol: ProtocolId,
}

// ── Protocol trait (paypunkd side) ───────────────────────────────────────────

/// Non-signer protocol operations: build unsigned artifacts, finalize signed
/// artifacts, validate addresses, query balances, and provide protocol metadata.
#[async_trait::async_trait]
pub trait Protocol: Send + Sync {
    fn protocol_id(&self) -> ProtocolId;

    // ── Transaction operations ──────────────────────────────────────────────
    async fn build(&self, intent: &Intent) -> Result<Vec<u8>, String>;
    /// Store a signed PCZT in the wallet database and finalize it,
    /// returning the raw transaction bytes ready for broadcast.
    async fn store_and_finalize(&self, signed_pczt: &[u8]) -> Result<Vec<u8>, String>;
    fn finalize(&self, signed: &[u8]) -> Result<Vec<u8>, String>;
    async fn broadcast(&self, finalized_tx: &[u8]) -> Result<String, String>;

    // ── Queries ─────────────────────────────────────────────────────────────
    fn validate_address(&self, address: &str) -> bool;
    async fn get_balance(&self, address: &str, asset: &str) -> Result<Balance, String>;

    // ── Protocol metadata ───────────────────────────────────────────────────
    fn chain_id(&self) -> ChainId;
    fn native_asset(&self) -> String;
    fn ticker(&self) -> &str;
    fn decimals(&self) -> u8;
    fn block_explorer_url(&self, tx_hash: &str) -> String;
    fn default_derivation_path(&self, account: u32) -> String;
    fn default_account_name(&self, account_index: u32) -> String;

    // ── Key operations ──────────────────────────────────────────────────────
    /// Derive an address from a viewing key.
    ///
    /// `index` is the address index within the account. For Ethereum this is
    /// typically ignored (one address per account); for Zcash it selects which
    /// diversifier to use within the account.
    fn derive_address_from_viewing_key(&self, vk: &[u8], index: u32) -> Result<String, String>;

    // ── Chain sync ──────────────────────────────────────────────────────────
    /// Get the current sync status.
    async fn get_sync_status(&self) -> Result<SyncStatus, String> {
        Err(format!(
            "sync status not supported for {:?}",
            self.protocol_id()
        ))
    }

    // ── Transfer operations ──────────────────────────────────────────────────
    /// Create a transfer for the given account.
    async fn create_transfer(
        &self,
        _account: u32,
        _to: String,
        _amount: u64,
        _memo: Option<String>,
    ) -> Result<Vec<u8>, String> {
        Err(format!(
            "create_transfer not supported for {:?}",
            self.protocol_id()
        ))
    }

    /// Estimate the fee for a transfer.
    async fn estimate_fee(
        &self,
        _to: String,
        _amount: u64,
        _memo: Option<String>,
    ) -> Result<u64, String> {
        Err(format!(
            "estimate_fee not supported for {:?}",
            self.protocol_id()
        ))
    }

    // ── History & status ────────────────────────────────────────────────────
    /// Fetch transaction history for the given account.
    async fn get_history(
        &self,
        _account: u32,
        _cursor: Option<String>,
        _limit: u32,
    ) -> Result<Page<HistoryEntry>, String> {
        Ok(Page {
            items: vec![],
            next_cursor: None,
            has_more: false,
        })
    }

    /// Get the on-chain status of a transaction.
    async fn get_transaction_status(&self, _txid: String) -> Result<TxStatus, String> {
        Err(format!(
            "get_transaction_status not supported for {:?}",
            self.protocol_id()
        ))
    }

    /// Get the current block height.
    async fn get_current_block_height(
        &self,
        _lightwalletd_host: String,
    ) -> Result<BlockHeight, String> {
        Err(format!(
            "get_current_block_height not supported for {:?}",
            self.protocol_id()
        ))
    }

    /// Register a newly created account and sync it.
    ///
    /// Called after `create_account` so the protocol can import the viewing key
    /// into its own database and scan the chain for the account's notes.
    async fn sync_account(
        &self,
        _viewing_key: &[u8],
        _birthday_height: u64,
        _address: &str,
    ) -> Result<(), String> {
        Ok(())
    }
}

// ── SignerProtocol trait (keypunkd side) ─────────────────────────────────────

/// Signer-side protocol operations: export viewing keys, parse unsigned
/// artifacts for user preview, and sign artifacts.
#[async_trait::async_trait]
pub trait SignerProtocol: Send + Sync {
    async fn chain(&self) -> ChainId;
    fn export_viewing(&self, seed: &[u8; 64], path: &str) -> Result<Vec<u8>, String>;
    fn parse_artifact(&self, artifact: &[u8]) -> Result<Vec<u8>, String>;
    fn sign(&self, seed: &[u8; 64], path: &str, artifact: &[u8]) -> Result<Vec<u8>, String>;
}

// ── Protocol metadata ────────────────────────────────────────────────────────

/// Static metadata about a protocol, returned by the daemon for display/CLI use.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProtocolMetadata {
    pub id: ProtocolId,
    pub chain_id: String,
    pub native_asset: String,
    pub ticker: String,
    pub decimals: u8,
    pub block_explorer_template: String,
}

// ── Account ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Account {
    pub id: String,
    pub protocol: ProtocolId,
    pub derivation_path: String,
    pub name: String,
    pub address: String,
    pub viewing_key: Vec<u8>,
    pub created_at: u64,
}

// ── Data model ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Address(pub String);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Amount(pub u128);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TransferId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlockHeight(pub u64);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Balance {
    pub spendable: Amount,
    pub pending: Amount,
    pub total: Amount,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TransactionStatus {
    Pending,
    Confirmed(BlockHeight),
    Failed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Transfer {
    pub id: TransferId,
    pub from: Address,
    pub to: Address,
    pub amount: Amount,
    pub fee: Amount,
    pub memo: Option<String>,
    pub status: TransactionStatus,
    pub created_at: u64,
}

// ── Reference API supporting types ──────────────────────────────────────────

/// Paginated response wrapper.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

/// A single entry in transaction history.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HistoryEntry {
    pub hash: String,
    pub direction: TxDirection,
    pub counterparty: Address,
    pub amount: Amount,
    pub status: TxStatus,
    pub timestamp: Option<u64>,
}

/// Whether a history entry is incoming, outgoing, or a self-transfer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TxDirection {
    Incoming,
    Outgoing,
    SelfTransfer,
}

/// On-chain transaction status for display / query purposes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TxStatus {
    Pending,
    Confirmed { confirmations: u64 },
    Failed { reason: String },
    NotFound,
}

/// A single UTXO (unspent transaction output).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Utxo {
    pub tx_hash: String,
    pub output_index: u32,
    pub amount: Amount,
    pub address: Address,
    pub confirmations: u64,
    pub is_shielded: bool,
}

/// A payment proof that can be shared with a recipient.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaymentProof(pub Vec<u8>);

/// Status of a chain sync operation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct SyncStatus {
    pub is_syncing: bool,
    pub current_height: u64,
    pub target_height: u64,
}
