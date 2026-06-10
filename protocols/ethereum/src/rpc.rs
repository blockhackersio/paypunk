/// Trait abstracting an Ethereum JSON-RPC client for balance queries.
///
/// Implementations can use any transport (HTTP, IPC, WebSocket) and
/// any RPC library (reqwest, alloy-provider, ethers-rs, etc.).
pub trait EthRpcClient: Send + Sync {
    /// Query the ETH balance of the given address (in wei).
    fn get_eth_balance(&self, address: &str) -> Result<u64, String>;

    /// Query the ERC-20 token balance of the given address (in the
    /// smallest token unit) for the specified token contract.
    fn get_erc20_balance(&self, address: &str, token_address: &str) -> Result<u64, String>;
}

/// No-op implementation for contexts that only need signing
/// (e.g. keypunkd where balance queries are never called).
impl EthRpcClient for () {
    fn get_eth_balance(&self, _address: &str) -> Result<u64, String> {
        Err("no RPC client configured".to_string())
    }

    fn get_erc20_balance(&self, _address: &str, _token_address: &str) -> Result<u64, String> {
        Err("no RPC client configured".to_string())
    }
}

/// A stub client that always returns "not implemented".
/// Use this as a placeholder until a real RPC client is wired.
pub struct UnimplementedRpcClient;

impl EthRpcClient for UnimplementedRpcClient {
    fn get_eth_balance(&self, _address: &str) -> Result<u64, String> {
        Err("ETH balance query not yet implemented — needs RPC endpoint".to_string())
    }

    fn get_erc20_balance(&self, _address: &str, _token_address: &str) -> Result<u64, String> {
        Err("ERC-20 balance query not yet implemented — needs RPC endpoint".to_string())
    }
}
