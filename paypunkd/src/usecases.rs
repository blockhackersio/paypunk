use keypunkd::services::KeypunkService;
use paypunk_types::{Balance, Intent, ProtocolId};

use crate::protocol_service::ProtocolService;

// ── Keypunkd forwarding ────────────────────────────────────────────────────

pub async fn get_keypunk_encryption_key(service: &KeypunkService) -> Result<[u8; 32], String> {
    service.get_encryption_key().await
}

pub async fn generate_seed(
    service: &KeypunkService,
    encrypted_password: Vec<u8>,
    client_public_key: [u8; 32],
) -> Result<Vec<u8>, String> {
    service
        .generate_seed(encrypted_password, client_public_key)
        .await
}

pub async fn restore_seed(
    service: &KeypunkService,
    encrypted_mnemonic: Vec<u8>,
    encrypted_password: Vec<u8>,
    client_public_key: [u8; 32],
) -> Result<(), String> {
    service
        .restore_seed(encrypted_mnemonic, encrypted_password, client_public_key)
        .await
}

pub async fn export_viewing_key(
    service: &KeypunkService,
    encrypted_password: Vec<u8>,
    client_public_key: [u8; 32],
    protocol: ProtocolId,
    account: u32,
) -> Result<Vec<u8>, String> {
    service
        .export_viewing_key(encrypted_password, client_public_key, protocol, account)
        .await
}

/// Submit an intent: build the unsigned artifact via the protocol,
/// then forward to keypunkd for parsing and preview.
pub async fn submit_intent(
    keypunk_service: &KeypunkService,
    protocols: &ProtocolService,
    intent: &Intent,
    derivation_path: &[u8],
) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>, [u8; 32]), String> {
    // Determine protocol from intent
    let protocol_id = match intent {
        Intent::Zcash(_) => ProtocolId::Zcash,
        Intent::Ethereum(_) => ProtocolId::Ethereum,
    };

    // Build the unsigned artifact
    let protocol = protocols.get(protocol_id)?;
    let raw_artifact = protocol.build(intent)?;

    // Forward to keypunkd for parsing and preview
    let preview = keypunk_service
        .preview_artifact(raw_artifact, protocol_id, derivation_path.to_vec())
        .await?;

    match preview {
        keypunkd::messages::KeypunkdResponse::ArtifactPreview {
            raw_artifact,
            parsed_summary,
            signature,
            keypunkd_public_key,
        } => Ok((raw_artifact, parsed_summary, signature, keypunkd_public_key)),
        keypunkd::messages::KeypunkdResponse::Error { message } => Err(message),
        _ => Err("unexpected response from keypunkd".to_string()),
    }
}

/// Approve and sign an artifact.
pub async fn approve_signature(
    keypunk_service: &KeypunkService,
    encrypted_payload: Vec<u8>,
    ephemeral_public_key: [u8; 32],
    derivation_path: Vec<u8>,
) -> Result<Vec<u8>, String> {
    keypunk_service
        .authorize_artifact(encrypted_payload, ephemeral_public_key, derivation_path)
        .await
}

// ── Local protocol operations ──────────────────────────────────────────────

/// Finalize a signed artifact into broadcast-ready bytes.
pub fn finalize_artifact(
    protocols: &ProtocolService,
    protocol: ProtocolId,
    signed: &[u8],
) -> Result<Vec<u8>, String> {
    protocols.get(protocol)?.finalize(signed)
}

/// Derive an address from a viewing key using the protocol service.
pub fn derive_address(
    _protocols: &ProtocolService,
    protocol: ProtocolId,
    viewing_key: &[u8],
    index: u32,
) -> Result<String, String> {
    match protocol {
        ProtocolId::Zcash => {
            paypunk_chains_zcash::address::derive_from_fvk(viewing_key, index)
                .map_err(|e| e.to_string())
        }
        ProtocolId::Ethereum => {
            paypunk_chains_ethereum::address::derive_from_pubkey(viewing_key)
                .map(|a| a.to_string())
                .map_err(|e| e.to_string())
        }
        _ => Err(format!("unsupported protocol for address derivation: {protocol:?}")),
    }
}

/// Validate an address using the protocol service.
pub fn validate_address(
    protocols: &ProtocolService,
    protocol: ProtocolId,
    address: &str,
) -> bool {
    protocols
        .get(protocol)
        .map(|p| p.validate_address(address))
        .unwrap_or(false)
}

// ── Stubs: depend on future work ───────────────────────────────────────────

/// Query the spendable, pending, and total balance for the given address and asset.
pub fn get_balance(
    protocols: &ProtocolService,
    protocol: ProtocolId,
    address: &str,
    asset: &str,
) -> Result<Balance, String> {
    protocols.get(protocol)?.get_balance(address, asset)
}

pub async fn create_transfer(
    _service: &KeypunkService,
    _protocols: &ProtocolService,
    _protocol: ProtocolId,
    _account: u32,
    _to: &str,
    _amount: u64,
    _memo: Option<&str>,
) -> Result<String, String> {
    todo!("create_transfer: PCZT pipeline not yet implemented — needs TransactionProposer")
}

pub async fn get_history(
    _protocol: ProtocolId,
    _account: u32,
    _cursor: Option<String>,
    _limit: u32,
) -> Result<String, String> {
    todo!("get_history: needs Page/HistoryEntry types")
}

pub async fn sync_wallet(_protocol: ProtocolId, _account: u32) -> Result<(), String> {
    todo!("sync_wallet: needs LSP/lightwalletd connection")
}

pub fn broadcast_transaction(
    protocols: &ProtocolService,
    protocol: ProtocolId,
    raw_tx: &[u8],
) -> Result<String, String> {
    let finalized = protocols.get(protocol)?.finalize(raw_tx)?;
    protocols.get(protocol)?.broadcast(&finalized)
}

pub async fn get_transaction_status(
    _protocol: ProtocolId,
    _txid: String,
) -> Result<paypunk_types::TxStatus, String> {
    todo!("get_transaction_status: needs lightwalletd/RPC client")
}

pub async fn get_current_block_height(_protocol: ProtocolId) -> Result<paypunk_types::BlockHeight, String> {
    todo!("get_current_block_height: needs lightwalletd/RPC client")
}

pub async fn estimate_fee(
    _protocol: ProtocolId,
    _to: &str,
    _amount: u64,
    _memo: Option<&str>,
) -> Result<u64, String> {
    todo!("estimate_fee: needs TransactionProposer + chain fee estimation")
}
