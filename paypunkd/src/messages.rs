use paypunk_types::{Account, Balance, Intent, ProtocolId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum PaypunkdRequest {
    GetKeypunkEncryptionKey,
    GenerateSeed {
        encrypted_password: Vec<u8>,
        client_public_key: [u8; 32],
    },
    RestoreSeed {
        encrypted_mnemonic: Vec<u8>,
        encrypted_password: Vec<u8>,
        client_public_key: [u8; 32],
    },
    SubmitIntent {
        intent: Intent,
        derivation_path: Vec<u8>,
    },
    ApproveSignature {
        encrypted_payload: Vec<u8>,
        ephemeral_public_key: [u8; 32],
        derivation_path: Vec<u8>,
    },
    DeriveAddress {
        encrypted_password: Vec<u8>,
        client_public_key: [u8; 32],
        protocol: ProtocolId,
        account: String,
        index: u32,
    },
    GetBalance {
        address: String,
        asset: String,
    },
    BroadcastTransaction {
        protocol: ProtocolId,
        raw_tx: Vec<u8>,
    },
    CreateAccount {
        protocol: ProtocolId,
        derivation_path: String,
        account_index: u32,
        name: String,
    },
    ListAccounts,
    GetAccount { id: String },
    GetPaypunkdEncryptionKey,
    HasSeed,
    Unlock {
        encrypted_db_password: Vec<u8>,
        ephemeral_public_key: [u8; 32],
        encrypted_keypunkd_password: Vec<u8>,
        keypunkd_client_pk: [u8; 32],
    },
    BulkDeriveAccounts {
        encrypted_password: Vec<u8>,
        client_public_key: [u8; 32],
        count: u32,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum PaypunkdResponse {
    KeypunkEncryptionKey { key: [u8; 32] },
    SeedGenerated { encrypted_mnemonic: Vec<u8> },
    SeedRestored,
    SignablePreview {
        raw_artifact: Vec<u8>,
        parsed_summary: Vec<u8>,
        keypunkd_signature: Vec<u8>,
        keypunkd_public_key: [u8; 32],
    },
    SignatureApproved { signed_artifact: Vec<u8> },
    Balance { balance: Balance },
    AddressDerived { address: String },
    TransactionBroadcasted { tx_hash: String },
    AccountCreated { account: Account },
    AccountsList { accounts: Vec<Account> },
    AccountFound { account: Option<Account> },
    PaypunkdEncryptionKey { key: [u8; 32] },
    HasSeed { exists: bool },
    UnlockSuccess { accounts_count: u32 },
    AccountsBulkDerived { accounts: Vec<Account> },
    Error { message: String },
}
