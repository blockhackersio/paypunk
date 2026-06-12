use paypunk_types::{Balance, Intent, ProtocolId};
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
    Unlock {
        encrypted_password: Vec<u8>,
        client_public_key: [u8; 32],
    },
    Lock,
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
        protocol: ProtocolId,
        account: String,
        index: u32,
    },
    GetBalance {
        address: String,
        asset: String,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum PaypunkdResponse {
    KeypunkEncryptionKey { key: [u8; 32] },
    SeedGenerated { encrypted_mnemonic: Vec<u8> },
    SeedRestored,
    Unlocked,
    Locked,
    SignablePreview {
        raw_artifact: Vec<u8>,
        parsed_summary: Vec<u8>,
        keypunkd_signature: Vec<u8>,
        keypunkd_public_key: [u8; 32],
    },
    SignatureApproved { signed_artifact: Vec<u8> },
    Balance { balance: Balance },
    AddressDerived { address: String },
    Error { message: String },
}
