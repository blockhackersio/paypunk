use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ProtocolId {
    Zcash,
    Bitcoin,
    Ethereum,
    Monero,
    Solana,
}

/// A protocol-specific key derivation and signing strategy.
///
/// Each protocol crate (zcash, bitcoin, ethereum, etc.) implements this trait
/// to provide derivation and signing logic that operates on the raw seed
/// inside keypunkd's protected memory. Private key material never leaves
/// that process.
pub trait Protocol: Send + Sync {
    fn protocol_id(&self) -> ProtocolId;
    /// Derive non-sensitive public key material (FVK, pubkey, xpub)
    /// for the given account. Private key material is NEVER included in the output.
    fn derive_public_key(&self, seed: &[u8; 64], account: u32) -> Result<Vec<u8>, String>;
    /// Derive an address from public key bytes at the given diversifier index.
    fn derive_address(&self, public_key: &[u8], index: u32) -> Result<String, String>;
    /// Sign a message with the derived private key at the given account.
    fn sign(&self, seed: &[u8; 64], account: u32, message: &[u8]) -> Result<Vec<u8>, String>;
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
