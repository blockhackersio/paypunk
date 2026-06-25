pub mod address;
pub mod protocol;
pub mod rpc;

pub use rpc::{EthRpcClient, HttpRpcClient, TxReceipt, UnimplementedRpcClient};

/// Return the standard Ethereum derivation path for a given account index.
///
/// Uses Metamask-style BIP44 derivation where each account is a sequential
/// address index under the first BIP44 account (account 0).
///
/// Path: `m/44'/60'/0'/0/{index}`
pub fn derivation_path(account: u32) -> String {
    format!("m/44'/60'/0'/0/{account}")
}
