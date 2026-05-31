use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum KeypunkdRequest {
    /// Fetch keypunkd's public key.
    GetPublicKey,
    /// Generate and persist a new wallet seed.
    GenerateSeed {
        /// Password encrypted to keypunkd's public key.
        /// Format: nonce(12) + ciphertext
        encrypted_password: Vec<u8>,
        /// Client's ephemeral X25519 public key (32 bytes).
        /// Used to derive the shared secret for both directions.
        client_public_key: [u8; 32],
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum KeypunkdResponse {
    PublicKey { key: [u8; 32] },
    SeedGenerated {
        /// Mnemonic encrypted to the client's ephemeral public key.
        /// Format: nonce(12) + ciphertext
        encrypted_mnemonic: Vec<u8>,
    },
    Error { message: String },
}
