use paypunk_types::AssetId;

/// Trait abstracting an Ethereum JSON-RPC client for balance queries.
///
/// Implementations can use any transport (HTTP, IPC, WebSocket) and
/// any RPC library (reqwest, alloy-provider, ethers-rs, etc.).
pub trait EthRpcClient: Send + Sync {
    /// Query the balance for the given address and asset.
    ///
    /// - `AssetId::Native` → ETH balance (wei)
    /// - `AssetId::Token(contract)` → ERC-20 token balance (smallest unit)
    fn get_balance(&self, address: &str, asset: &AssetId) -> Result<u64, String>;
}

/// No-op implementation for contexts that only need signing
/// (e.g. keypunkd where balance queries are never called).
impl EthRpcClient for () {
    fn get_balance(&self, _address: &str, _asset: &AssetId) -> Result<u64, String> {
        Err("no RPC client configured".to_string())
    }
}

/// A stub client that always returns "not implemented".
/// Use this as a placeholder until a real RPC client is wired.
pub struct UnimplementedRpcClient;

impl EthRpcClient for UnimplementedRpcClient {
    fn get_balance(&self, _address: &str, _asset: &AssetId) -> Result<u64, String> {
        Err("balance query not yet implemented — needs RPC endpoint".to_string())
    }
}
