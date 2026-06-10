use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ProtocolId {
    Zcash,
    Bitcoin,
    Ethereum,
    Monero,
    Solana,
}

/// Crypto operations only, no DB access.
pub trait Protocol: Send + Sync {
    fn protocol_id(&self) -> ProtocolId;
    fn derive_address(&self, public_key: &[u8], index: u32) -> Result<String, String>;
    fn validate_address(&self, address: &str) -> bool;
    fn finalize_transaction(&self, transaction: &[u8]) -> Result<Vec<u8>, String>;
    fn create_transaction(
        &self,
        public_key: &[u8],
        account: u32,
        to: &str,
        amount: u64,
        memo: Option<&str>,
    ) -> Result<Vec<u8>, String>;

    /// Query the balance for the given account.
    fn get_balance(&self, account: u32, public_key: &[u8]) -> Result<Balance, String>;
}

/// Signer-side protocol: key derivation and transaction signing.
/// Lives inside keypunkd — the security boundary. Never exposes raw key material.
pub trait SignerProtocol: Send + Sync {
    fn protocol_id(&self) -> ProtocolId;
    fn derive_public_key(&self, seed: &[u8; 64], account: u32) -> Result<Vec<u8>, String>;
    fn sign_transaction(
        &self,
        seed: &[u8; 64],
        account: u32,
        transaction: &[u8],
    ) -> Result<Vec<u8>, String>;
}

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
