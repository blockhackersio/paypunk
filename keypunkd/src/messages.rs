use paypunk_types::ProtocolId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum KeypunkdRequest {
    /// Fetch keypunkd's X25519 encryption key.
    GetEncryptionKey,
    /// Generate and persist a new wallet seed.
    GenerateSeed {
        encrypted_password: Vec<u8>,
        client_public_key: [u8; 32],
    },
    /// Restore a wallet from an existing mnemonic seed phrase.
    RestoreSeed {
        encrypted_mnemonic: Vec<u8>,
        encrypted_password: Vec<u8>,
        client_public_key: [u8; 32],
    },
    /// Parse an unsigned artifact and return a human-readable summary.
    PreviewArtifact {
        raw_artifact: Vec<u8>,
        protocol: ProtocolId,
        derivation_path: Vec<u8>,
    },
    /// Authorize and sign an artifact after user approval.
    AuthorizeArtifact {
        encrypted_payload: Vec<u8>,
        ephemeral_public_key: [u8; 32],
        derivation_path: Vec<u8>,
    },
    /// Export chain-specific viewing key material for the given path.
    ExportViewingKey {
        encrypted_password: Vec<u8>,
        client_public_key: [u8; 32],
        protocol: ProtocolId,
        account: u32,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum KeypunkdResponse {
    EncryptionKey {
        key: [u8; 32],
    },
    SeedGenerated {
        encrypted_mnemonic: Vec<u8>,
    },
    SeedRestored,
    ArtifactPreview {
        raw_artifact: Vec<u8>,
        parsed_summary: Vec<u8>,
        signature: Vec<u8>,
        keypunkd_public_key: [u8; 32],
    },
    ArtifactAuthorized {
        signed_artifact: Vec<u8>,
    },
    ViewingKey {
        key: Vec<u8>,
    },
    Error {
        message: String,
    },
}
