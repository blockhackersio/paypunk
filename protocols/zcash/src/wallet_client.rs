use tactix::{Recipient, Sender};

use crate::wallet_actor::WalletMessage;

/// Client that sends `WalletMessage`s to the `WalletDbActor`.
/// Implements `TransactionProposer` for Zcash.
pub struct ZcashWalletClient {
    pub recipient: Recipient<WalletMessage>,
}

impl ZcashWalletClient {
    /// Send a ProposeAndBuild message to the WalletDbActor and await the response.
    pub async fn create_transaction_async(
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
