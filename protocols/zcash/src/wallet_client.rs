use paypunk_types::TransactionProposer;
use tactix::{Recipient, Sender};

use crate::wallet_actor::WalletMessage;

/// Client that sends `WalletMessage`s to the `WalletDbActor`.
/// Implements `TransactionProposer` for Zcash.
pub struct ZcashWalletClient {
    pub recipient: Recipient<WalletMessage>,
}

impl TransactionProposer for ZcashWalletClient {
    fn propose_and_build(
        &self,
        _public_key: &[u8],
        _account: u32,
        _to: &str,
        _amount: u64,
        _memo: Option<&str>,
    ) -> Result<Vec<u8>, String> {
        Err("TransactionProposer::propose_and_build requires a WalletDbActor — \
             use the Recipient-based flow in paypunkd"
            .to_string())
    }
}

impl ZcashWalletClient {
    /// Send a ProposeAndBuild message to the WalletDbActor and await the response.
    pub async fn propose_and_build_async(
        &self,
        public_key: Vec<u8>,
        account: u32,
        to: String,
        amount: u64,
        memo: Option<String>,
    ) -> Result<Vec<u8>, String> {
        let result: Result<Vec<u8>, String> = self
            .recipient
            .ask(WalletMessage::ProposeAndBuild {
                public_key,
                account,
                to,
                amount,
                memo,
            })
            .await;
        result
    }
}
