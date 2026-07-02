use tactix::{Recipient, Sender};

use paypunk_types::{Balance, SyncStatus};

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

    /// Trigger a sync for the given account.
    pub async fn sync(
        &self,
        fvk: Vec<u8>,
        birthday_height: u64,
        lightwalletd_host: String,
    ) -> Result<String, String> {
        let bytes: Vec<u8> = self.recipient
            .ask(WalletMessage::Sync { fvk, birthday_height, lightwalletd_host })
            .await?;
        String::from_utf8(bytes).map_err(|e| format!("sync response not valid UTF-8: {e}"))
    }

    /// Get the current sync status.
    pub async fn get_status(&self) -> Result<SyncStatus, String> {
        let bytes = self.recipient
            .ask(WalletMessage::GetStatus)
            .await?;
        postcard::from_bytes(&bytes)
            .map_err(|e| format!("deserialize status failed: {e}"))
    }

    /// Get the wallet balance.
    pub async fn get_balance(&self) -> Result<Balance, String> {
        let bytes = self.recipient
            .ask(WalletMessage::GetBalance)
            .await?;
        postcard::from_bytes(&bytes)
            .map_err(|e| format!("deserialize balance failed: {e}"))
    }
}
