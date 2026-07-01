use zcash_client_backend::proto::service::compact_tx_streamer_client::CompactTxStreamerClient;
use zcash_client_backend::proto::service::{ChainSpec, RawTransaction};
use zcash_client_sqlite::WalletDb;
use zcash_protocol::consensus::{BlockHeight, Network};
use rand_core::OsRng;
use zcash_client_sqlite::util::SystemClock;
use tonic::transport::Channel;

/// Lightwalletd gRPC client for Zcash chain interaction.
pub struct LspClient {
    inner: CompactTxStreamerClient<Channel>,
    params: Network,
}

impl LspClient {
    /// Connect to a lightwalletd endpoint.
    pub async fn connect(host: &str, params: Network) -> Result<Self, String> {
        let inner = CompactTxStreamerClient::connect(host.to_string())
            .await
            .map_err(|e| format!("failed to connect to lightwalletd: {e}"))?;
        Ok(Self { inner, params })
    }

    /// Get the latest block height from lightwalletd.
    pub async fn get_latest_height(&mut self) -> Result<BlockHeight, String> {
        let info = self
            .inner
            .get_latest_block(ChainSpec::default())
            .await
            .map_err(|e| format!("lightwalletd get_latest_block failed: {e}"))?;
        let height = info.get_ref().height as u32;
        Ok(BlockHeight::from_u32(height))
    }

    /// Broadcast a raw transaction to the network.
    pub async fn broadcast_tx(&mut self, tx_bytes: &[u8]) -> Result<String, String> {
        let response = self
            .inner
            .send_transaction(RawTransaction { data: tx_bytes.to_vec(), height: 0 })
            .await
            .map_err(|e| format!("broadcast failed: {e}"))?;
        let result = response.into_inner();
        if result.error_code != 0 {
            return Err(format!("broadcast failed ({}): {}", result.error_code, result.error_message));
        }
        Ok("broadcast successful".to_string())
    }

    /// Scan a range of blocks into the WalletDb.
    /// Returns (scanned_from, scanned_to) heights.
    pub fn scan_range(
        &self,
        wallet_db: &WalletDb<rusqlite::Connection, Network, SystemClock, OsRng>,
        from_height: BlockHeight,
        to_height: BlockHeight,
    ) -> Result<(u64, u64), String> {
        let from: u64 = from_height.into();
        let to: u64 = to_height.into();

        let mut current = from;
        let batch_size = 100;

        while current < to {
            let batch_end = (current + batch_size).min(to);
            let _ = wallet_db;
            let _ = batch_end;
            // TODO: Use zcash_client_backend::sync or scan_cached_blocks
            // to process blocks from lightwalletd into the WalletDb.
            // This requires the FVK to be registered and the WalletDb
            // to be initialized.
            current = batch_end;
        }

        Ok((from, current))
    }
}
