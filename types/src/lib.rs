use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ProtocolId {
    Zcash,
    Bitcoin,
    Ethereum,
    Monero,
    Solana,
}

/// Object-safe: crypto operations only, no DB access.
pub trait Protocol: Send + Sync {
    fn protocol_id(&self) -> ProtocolId;
    fn derive_address(&self, public_key: &[u8], index: u32) -> Result<String, String>;
    fn prove_transaction(&self, transaction: &[u8]) -> Result<Vec<u8>, String>;
    fn finalize_transaction(&self, transaction: &[u8]) -> Result<Vec<u8>, String>;
}

/// NOT object-safe: has associated type `TransactionProposer`.
/// Extends `Protocol` with the ability to propose and build an unsigned
/// transaction from wallet state.
pub trait ProposingProtocol: Protocol {
    type TransactionProposer: TransactionProposer;

    fn propose_and_build(
        &self,
        proposer: &Self::TransactionProposer,
        public_key: &[u8],
        account: u32,
        to: &str,
        amount: u64,
        memo: Option<&str>,
    ) -> Result<Vec<u8>, String>;
}

/// The proposition-only interface. Each chain provides an implementation
/// that communicates with its wallet database actor.
pub trait TransactionProposer: Send + Sync {
    fn propose_and_build(
        &self,
        public_key: &[u8],
        account: u32,
        to: &str,
        amount: u64,
        memo: Option<&str>,
    ) -> Result<Vec<u8>, String>;
}

/// Signer-side protocol: key derivation and transaction signing.
/// Lives inside keypunkd — the security boundary. Never exposes raw key material.
pub trait SignerProtocol: Send + Sync {
    fn protocol_id(&self) -> ProtocolId;
    fn derive_public_key(&self, seed: &[u8; 64], account: u32) -> Result<Vec<u8>, String>;
    fn sign_transaction(&self, seed: &[u8; 64], account: u32, transaction: &[u8]) -> Result<Vec<u8>, String>;
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
