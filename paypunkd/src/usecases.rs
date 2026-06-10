use keypunkd::services::KeypunkService;
use paypunk_types::{AssetId, Balance, BlockHeight, ProtocolId, TxStatus};

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

pub async fn unlock(
    service: &KeypunkService,
    encrypted_password: Vec<u8>,
    client_public_key: [u8; 32],
) -> Result<(), String> {
    service.unlock(encrypted_password, client_public_key).await
}

pub async fn derive_public_key(
    service: &KeypunkService,
    protocol: ProtocolId,
    account: u32,
) -> Result<Vec<u8>, String> {
    service.derive_public_key(protocol, account).await
}

pub async fn sign(
    service: &KeypunkService,
    protocol: ProtocolId,
    account: u32,
    payload: Vec<u8>,
) -> Result<Vec<u8>, String> {
    service.sign(protocol, account, payload).await
}

pub async fn lock(service: &KeypunkService) -> Result<(), String> {
    service.lock().await
}

// ── Local protocol operations ──────────────────────────────────────────────

/// Derive an address from a cached public key using the protocol service.
///
/// Callers are responsible for fetching and caching the public key via
/// `derive_public_key` before calling this function.
pub fn derive_address(
    protocols: &ProtocolService,
    protocol: ProtocolId,
    public_key: &[u8],
    index: u32,
) -> Result<String, String> {
    protocols.get(protocol)?.derive_address(public_key, index)
}

/// Finalize a signed transaction using the protocol service.
///
/// For Zcash this combines proven + signed PCZTs, finalizes spends,
/// and extracts the raw transaction bytes.
pub fn finalize_transaction(
    protocols: &ProtocolService,
    protocol: ProtocolId,
    transaction: &[u8],
) -> Result<Vec<u8>, String> {
    protocols
        .get(protocol)?
        .finalize_transaction(transaction)
}

// ── Stubs: depend on future work ───────────────────────────────────────────

/// Full PCZT pipeline orchestration:
/// 1. Fetch public key from keypunkd
/// 2. create_transaction (includes proving, no separate step needed)
/// 3. sign via keypunkd IPC
/// 4. finalize_transaction
/// 5. store transaction
/// 6. return txid
///
/// TODO: Needs `TransactionProposer` (requires chain-specific wallet DB setup
/// in paypunkd) for storing transactions.
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

/// Query the spendable, pending, and total balance for the given protocol
/// and account.
///
/// Delegates to the chain-specific `Protocol::get_balance` implementation.
/// For chains without a wallet DB or RPC endpoint wired yet, the default
/// trait implementation returns a zero balance.
pub fn get_balance(
    protocols: &ProtocolService,
    protocol: ProtocolId,
    account: u32,
    public_key: &[u8],
    asset: &AssetId,
) -> Result<Balance, String> {
    protocols.get(protocol)?.get_balance(account, public_key, asset)
}

/// Fetch paginated transaction history for the given protocol and account.
///
/// TODO: Needs the `Page<T>` / `HistoryEntry`
/// types from the reference API (not yet added to paypunk-types).
pub async fn get_history(
    _protocol: ProtocolId,
    _account: u32,
    _cursor: Option<String>,
    _limit: u32,
) -> Result<String, String> {
    todo!("get_history: needs Page/HistoryEntry types")
}

/// Trigger a chain scan to detect incoming payments.
///
/// TODO: Needs LSP/lightwalletd client integration.
pub async fn sync_wallet(_protocol: ProtocolId, _account: u32) -> Result<(), String> {
    todo!("sync_wallet: needs LSP/lightwalletd connection")
}

/// Broadcast a signed and finalized transaction to the network.
///
/// TODO: Needs a lightwalletd gRPC client or RPC endpoint for submission.
pub async fn broadcast_transaction(
    _protocol: ProtocolId,
    _raw_tx: Vec<u8>,
) -> Result<String, String> {
    todo!("broadcast_transaction: needs lightwalletd/RPC client")
}

/// Query the on-chain status of a transaction by its txid.
///
/// TODO: Needs lightwalletd client or chain RPC to look up tx status.
pub async fn get_transaction_status(
    _protocol: ProtocolId,
    _txid: String,
) -> Result<TxStatus, String> {
    todo!("get_transaction_status: needs lightwalletd/RPC client")
}

/// Return the current block height of the network.
///
/// TODO: Needs lightwalletd client or chain RPC.
pub async fn get_current_block_height(_protocol: ProtocolId) -> Result<BlockHeight, String> {
    todo!("get_current_block_height: needs lightwalletd/RPC client")
}

/// Estimate the fee for a proposed transfer.
///
/// TODO: Needs `TransactionProposer` to build an unsigned tx and query fee
/// estimates from the chain.
pub async fn estimate_fee(
    _protocol: ProtocolId,
    _to: &str,
    _amount: u64,
    _memo: Option<&str>,
) -> Result<u64, String> {
    todo!("estimate_fee: needs TransactionProposer + chain fee estimation")
}
