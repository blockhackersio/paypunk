use paypunk_types::ProtocolId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum PaypunkdRequest {
    GetKeypunkPublicKey,
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
    DeriveAddress {
        protocol: ProtocolId,
        account: u32,
        index: u32,
    },
    Sign {
        protocol: ProtocolId,
        account: u32,
        payload: Vec<u8>,
    },
    Lock,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum PaypunkdResponse {
    KeypunkPublicKey { key: [u8; 32] },
    SeedGenerated { encrypted_mnemonic: Vec<u8> },
    SeedRestored,
    Unlocked,
    AddressDerived { address: String },
    Signature { signature: Vec<u8> },
    Locked,
    Error { message: String },
}
