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
    /// Restore a wallet from an existing mnemonic seed phrase.
    RestoreSeed {
        /// Mnemonic encrypted to keypunkd's public key.
        /// Format: nonce(12) + ciphertext
        encrypted_mnemonic: Vec<u8>,
        /// Password encrypted to keypunkd's public key.
        /// Format: nonce(12) + ciphertext
        encrypted_password: Vec<u8>,
        /// Client's ephemeral X25519 public key (32 bytes).
        client_public_key: [u8; 32],
    },
    /// Unlock the wallet: decrypt the seed and hold it in memory for the session.
    Unlock {
        /// Password encrypted to keypunkd's public key.
        encrypted_password: Vec<u8>,
        /// Client's ephemeral X25519 public key.
        client_public_key: [u8; 32],
    },
    /// Derive a Zcash address at the given diversifier index.
    /// Requires an active unlocked session.
    DeriveAddress {
        /// Diversifier index for the address.
        index: u32,
    },
    /// Lock the wallet: zero the in-memory seed and end the session.
    Lock,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum KeypunkdResponse {
    PublicKey {
        key: [u8; 32],
    },
    SeedGenerated {
        /// Mnemonic encrypted to the client's ephemeral public key.
        /// Format: nonce(12) + ciphertext
        encrypted_mnemonic: Vec<u8>,
    },
    SeedRestored,
    Unlocked,
    AddressDerived {
        /// The derived unified Zcash address string.
        address: String,
    },
    Locked,
    Error {
        message: String,
    },
}
