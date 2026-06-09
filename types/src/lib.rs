use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ProtocolId {
    Zcash,
    Bitcoin,
    Ethereum,
    Monero,
    Solana,
}

/// A protocol-specific non-signer strategy for address derivation, transaction
/// building, proving, and finalizing.
///
/// Lives inside paypunkd — never holds key material. Each protocol crate
/// (zcash, bitcoin, ethereum, etc.) implements this trait to provide
/// chain-specific logic for non-secret wallet operations.
pub trait Protocol: Send + Sync {
    fn protocol_id(&self) -> ProtocolId;
    /// Derive an address from public key bytes at the given diversifier index.
    fn derive_address(&self, public_key: &[u8], index: u32) -> Result<String, String>;
    /// Build an unsigned transaction from a transfer proposal.
    ///
    /// For Zcash: selects notes from the repository, computes fees, and returns
    /// a serialized PCZT.
    /// For Ethereum: reads nonce and balance from the repository and returns an
    /// unsigned RLP-encoded transaction.
    fn propose_and_build(
        &self,
        public_key: &[u8],
        repository: &dyn WalletRepository,
        account: u32,
        to: &str,
        amount: u64,
        memo: Option<&str>,
    ) -> Result<Vec<u8>, String>;
    /// Create ZK proofs for the transaction. For non-ZK protocols this is a
    /// no-op that returns the transaction bytes unchanged.
    fn prove_transaction(&self, transaction: &[u8]) -> Result<Vec<u8>, String>;
    /// Finalize the transaction: finalize spends, verify, and extract raw
    /// transaction bytes ready for broadcast.
    ///
    /// For Zcash: takes a PCZT that has been both proved and signed, finalizes
    /// spends, and extracts the raw transaction.
    /// For Ethereum: the signed RLP transaction is returned as-is.
    fn finalize_transaction(&self, transaction: &[u8]) -> Result<Vec<u8>, String>;
}

/// Signer-side protocol: key derivation and transaction signing.
/// Lives inside keypunkd — the security boundary. Never exposes raw key material.
pub trait SignerProtocol: Send + Sync {
    fn protocol_id(&self) -> ProtocolId;
    /// Derive non-sensitive public key material (FVK, pubkey, xpub)
    /// for the given account. Private key material is NEVER included in the output.
    fn derive_public_key(&self, seed: &[u8; 64], account: u32) -> Result<Vec<u8>, String>;
    /// Sign a transaction. For Zcash: signs a PCZT. For Ethereum: signs a hash.
    /// Returns the transaction with signatures applied.
    fn sign_transaction(&self, seed: &[u8; 64], account: u32, transaction: &[u8]) -> Result<Vec<u8>, String>;
}

/// Chain-agnostic wallet state access.
pub trait WalletRepository: Send + Sync {
    fn get_balance(&self, account: u32) -> Result<Balance, String>;
    fn get_spendable_resources(&self, account: u32) -> Result<Vec<Vec<u8>>, String>;
    fn mark_resources_spent(&self, account: u32, txid: &str) -> Result<(), String>;
    fn store_transaction(&self, account: u32, txid: &str, raw_tx: &[u8]) -> Result<(), String>;
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
