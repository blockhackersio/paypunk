use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum PaypunkdRequest {
    GetKeypunkPublicKey,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum PaypunkdResponse {
    KeypunkPublicKey { key: [u8; 32] },
    Error { message: String },
}
