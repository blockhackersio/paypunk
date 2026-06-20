use keypunkd::services::KeypunkService;
use paypunk_types::{Account, Balance, Intent, ProtocolId};
use rand::Rng;
use std::collections::HashMap;

use crate::database::{AccountsRepository, Database};
use crate::protocol_service::ProtocolService;

// ── Keypunkd forwarding ────────────────────────────────────────────────────

/// Forward a GetEncryptionKey request to keypunkd and return its X25519 public key.
pub async fn get_keypunk_encryption_key(service: &KeypunkService) -> Result<[u8; 32], String> {
    service.get_encryption_key().await
}

/// Forward a HasSeed request to keypunkd.
pub async fn has_seed(service: &KeypunkService) -> Result<bool, String> {
    service.has_seed().await
}

/// Forward a GenerateSeed request to keypunkd with the encrypted password.
/// Returns the encrypted mnemonic from keypunkd.
pub async fn generate_seed(
    service: &KeypunkService,
    encrypted_password: Vec<u8>,
    client_public_key: [u8; 32],
) -> Result<Vec<u8>, String> {
    service
        .generate_seed(encrypted_password, client_public_key)
        .await
}

/// Forward a RestoreSeed request to keypunkd with the encrypted mnemonic and password.
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

/// Forward an ExportViewingKey request to keypunkd to derive viewing key material
/// for the given protocol and account index.
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
        ProtocolId::Zcash => paypunk_chains_zcash::address::derive_from_fvk(viewing_key, index)
            .map_err(|e| e.to_string()),
        ProtocolId::Ethereum => paypunk_chains_ethereum::address::derive_from_pubkey(viewing_key)
            .map(|a| a.to_string())
            .map_err(|e| e.to_string()),
        _ => Err(format!(
            "unsupported protocol for address derivation: {protocol:?}"
        )),
    }
}

/// Validate an address using the protocol service.
pub fn validate_address(protocols: &ProtocolService, protocol: ProtocolId, address: &str) -> bool {
    protocols
        .get(protocol)
        .map(|p| p.validate_address(address))
        .unwrap_or(false)
}

// ── Account operations ──────────────────────────────────────────────────────

/// Create a new account from a pre-derived viewing key (no keypunkd call).
/// Accounts must be pre-derived via unlock (indices 0-29).
pub async fn create_account(
    db: &Database,
    repo: &dyn AccountsRepository,
    pre_derived_keys: &HashMap<(ProtocolId, u32), Vec<u8>>,
    protocol: ProtocolId,
    derivation_path: String,
    account_index: u32,
    name: String,
) -> Result<Account, String> {
    let conn = db.conn.as_ref().ok_or("database is locked")?;
    let conn = conn.lock().map_err(|e| e.to_string())?;

    let existing = repo.find_by_protocol(&conn, &protocol)?;
    if existing.iter().any(|a| a.derivation_path == derivation_path) {
        return Err("account already exists".to_string());
    }
    drop(conn);

    if account_index > 29 {
        return Err(format!(
            "account index {account_index} is beyond pre-derived range (0-29). \
             Re-unlock with a higher count to access this account."
        ));
    }

    let viewing_key = pre_derived_keys
        .get(&(protocol, account_index))
        .cloned()
        .ok_or_else(|| {
            format!(
                "no pre-derived viewing key found for {protocol:?} account {account_index}. \
                 Generate seed and unlock first."
            )
        })?;

    let id: String = (0..16)
        .map(|_| {
            let hex = rand::thread_rng().gen_range(0..16);
            format!("{hex:x}")
        })
        .collect();

    let account = Account {
        id,
        protocol,
        derivation_path,
        name,
        viewing_key,
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    };

    let conn = db.conn.as_ref().ok_or("database is locked")?;
    let conn = conn.lock().map_err(|e| e.to_string())?;
    repo.save(&conn, &account)?;
    Ok(account)
}

/// Bulk-derive accounts for all registered protocols.
pub async fn bulk_derive_accounts(
    keypunk_service: &KeypunkService,
    db: &Database,
    repo: &dyn AccountsRepository,
    encrypted_password: Vec<u8>,
    client_public_key: [u8; 32],
    protocols: Vec<ProtocolId>,
    count: u32,
) -> Result<Vec<Account>, String> {
    let keys = keypunk_service
        .bulk_export_viewing_keys(encrypted_password, client_public_key, protocols.clone(), 0, count)
        .await?;

    let mut accounts = Vec::new();
    for (protocol, account_index, viewing_key) in keys {
        let id: String = (0..16)
            .map(|_| {
                let hex = rand::thread_rng().gen_range(0..16);
                format!("{hex:x}")
            })
            .collect();

        let coin_type = match protocol {
            ProtocolId::Zcash => 133,
            ProtocolId::Ethereum => 60,
            ProtocolId::Bitcoin => 0,
            ProtocolId::Monero => 128,
            ProtocolId::Solana => 501,
        };

        let account = Account {
            id,
            protocol,
            derivation_path: format!("m/44'/{coin_type}'/{account_index}'"),
            name: format!("{protocol:?} Account {account_index}"),
            viewing_key,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };

        let conn = db.conn.as_ref().ok_or("database is locked")?;
        let conn = conn.lock().map_err(|e| e.to_string())?;
        repo.save(&conn, &account)?;
        accounts.push(account);
    }

    Ok(accounts)
}

/// List all accounts from the database.
pub fn list_accounts(
    db: &Database,
    repo: &dyn AccountsRepository,
) -> Result<Vec<Account>, String> {
    let conn = db.conn.as_ref().ok_or("database is locked")?;
    let conn = conn.lock().map_err(|e| e.to_string())?;
    repo.find_all(&conn)
}

/// Get a single account by ID.
pub fn get_account(
    db: &Database,
    repo: &dyn AccountsRepository,
    id: &str,
) -> Result<Option<Account>, String> {
    let conn = db.conn.as_ref().ok_or("database is locked")?;
    let conn = conn.lock().map_err(|e| e.to_string())?;
    repo.find_by_id(&conn, id)
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

/// Create a transfer for the given protocol and account.
/// TODO: Requires PCZT pipeline — not yet implemented.
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

/// Fetch transaction history for the given protocol and account.
/// TODO: Requires Page/HistoryEntry types and chain backend — not yet implemented.
pub async fn get_history(
    _protocol: ProtocolId,
    _account: u32,
    _cursor: Option<String>,
    _limit: u32,
) -> Result<String, String> {
    todo!("get_history: needs Page/HistoryEntry types")
}

/// Sync the wallet state with the blockchain for the given protocol and account.
/// TODO: Requires LSP/lightwalletd connection — not yet implemented.
pub async fn sync_wallet(_protocol: ProtocolId, _account: u32) -> Result<(), String> {
    todo!("sync_wallet: needs LSP/lightwalletd connection")
}

/// Finalize and broadcast a signed transaction to the network.
/// Returns the transaction hash.
pub fn broadcast_transaction(
    protocols: &ProtocolService,
    protocol: ProtocolId,
    raw_tx: &[u8],
) -> Result<String, String> {
    let finalized = protocols.get(protocol)?.finalize(raw_tx)?;
    protocols.get(protocol)?.broadcast(&finalized)
}

/// Query the on-chain status of a transaction by its ID.
/// TODO: Requires lightwalletd/RPC client — not yet implemented.
pub async fn get_transaction_status(
    _protocol: ProtocolId,
    _txid: String,
) -> Result<paypunk_types::TxStatus, String> {
    todo!("get_transaction_status: needs lightwalletd/RPC client")
}

/// Get the current block height from the blockchain.
/// TODO: Requires lightwalletd/RPC client — not yet implemented.
pub async fn get_current_block_height(
    _protocol: ProtocolId,
) -> Result<paypunk_types::BlockHeight, String> {
    todo!("get_current_block_height: needs lightwalletd/RPC client")
}

/// Estimate the fee for a transfer to the given address with the given amount and optional memo.
/// TODO: Requires TransactionProposer + chain fee estimation — not yet implemented.
pub async fn estimate_fee(
    _protocol: ProtocolId,
    _to: &str,
    _amount: u64,
    _memo: Option<&str>,
) -> Result<u64, String> {
    todo!("estimate_fee: needs TransactionProposer + chain fee estimation")
}
