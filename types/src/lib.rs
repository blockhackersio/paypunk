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

/// Signer-side protocol: key derivation and transaction signing.
/// Lives inside keypunkd — the security boundary. Never exposes raw key material.
pub trait SignerProtocol: Send + Sync {
    fn protocol_id(&self) -> ProtocolId;
    /// Derive non-sensitive public key material (FVK, pubkey, xpub)
    /// for the given account. Private key material is NEVER included in the output.
    fn derive_public_key(&self, seed: &[u8; 64], account: u32) -> Result<Vec<u8>, String>;
    /// Sign a PCZT transaction. The transaction bytes are a serialized PCZT.
    /// Returns the PCZT with signatures applied.
    fn sign_transaction(&self, seed: &[u8; 64], account: u32, transaction: &[u8]) -> Result<Vec<u8>, String>;
}

/// Non-signer-side protocol: address derivation, transaction building, proving,
/// and finalizing. Lives inside paypunkd — never holds key material.
pub trait NonSignerProtocol: Send + Sync {
    fn protocol_id(&self) -> ProtocolId;
    /// Derive an address from public key bytes at the given diversifier index.
    fn derive_address(&self, public_key: &[u8], index: u32) -> Result<String, String>;
    /// Build a PCZT from a transfer proposal (selects notes, computes fees).
    fn propose_and_build(
        &self,
        public_key: &[u8],
        repository: &dyn WalletRepository,
        account: u32,
        to: &str,
        amount: u64,
        memo: Option<&str>,
    ) -> Result<Vec<u8>, String>;
    /// Create zk-SNARK proofs for the given PCZT. Orchard-only.
    fn prove_transaction(&self, transaction: &[u8]) -> Result<Vec<u8>, String>;
    /// Combine proven and signed PCZTs, finalize spends, extract raw transaction bytes.
    fn finalize_transaction(
        &self,
        transaction: &[u8],
        signed_transaction: &[u8],
    ) -> Result<Vec<u8>, String>;
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
