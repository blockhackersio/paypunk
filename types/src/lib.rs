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
    pub memo: Option<String>,
    pub protocol: ProtocolId,
}

// ── Protocol trait (paypunkd side) ───────────────────────────────────────────

/// Non-signer protocol operations: build unsigned artifacts, finalize signed
/// artifacts, validate addresses, and query balances.
pub trait Protocol: Send + Sync {
    fn protocol_id(&self) -> ProtocolId;
    fn build(&self, intent: &Intent) -> Result<Vec<u8>, String>;
    fn finalize(&self, signed: &[u8]) -> Result<Vec<u8>, String>;
    fn validate_address(&self, address: &str) -> bool;
    fn get_balance(&self, address: &str, asset: &str) -> Result<Balance, String>;
}

// ── SignerProtocol trait (keypunkd side) ─────────────────────────────────────

/// Signer-side protocol operations: export viewing keys, parse unsigned
/// artifacts for user preview, and sign artifacts.
pub trait SignerProtocol: Send + Sync {
    fn chain(&self) -> ChainId;
    fn export_viewing(&self, seed: &[u8; 64], path: &[u8]) -> Result<Vec<u8>, String>;
    fn parse_artifact(&self, artifact: &[u8]) -> Result<Vec<u8>, String>;
    fn sign(&self, seed: &[u8; 64], artifact: &[u8]) -> Result<Vec<u8>, String>;
}

// ── Data model ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Address(pub String);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Amount(pub u64);

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
