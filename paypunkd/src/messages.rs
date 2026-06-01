use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum PaypunkdRequest {
    GetKeypunkPublicKey,
    GenerateSeed {
        encrypted_password: Vec<u8>,
        client_public_key: [u8; 32],
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum PaypunkdResponse {
    KeypunkPublicKey { key: [u8; 32] },
    SeedGenerated { encrypted_mnemonic: Vec<u8> },
    Error { message: String },
}
