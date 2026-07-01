# Step 7: ZcashProtocol Backend

## Goal
Wire `wallet_client` into `ZcashProtocol` and implement `build()`, `get_balance()`, and `broadcast()`.

## Changes

### `protocols/zcash/src/protocol.rs`

1. Add `wallet_client` field:
```rust
use crate::wallet_client::ZcashWalletClient;

pub struct ZcashProtocol {
    pub params: zcash_protocol::consensus::Network,
    pub wallet_client: Option<ZcashWalletClient>,
    pub lightwalletd_host: Option<String>,
}
```

2. Update `build()`:
```rust
async fn build(&self, intent: &Intent) -> Result<Vec<u8>, String> {
    match intent {
        Intent::Zcash(ZcashIntent::Transfer {
            to, amount, from, memo, ..
        }) => {
            if !self.validate_address(from) {
                return Err(format!("invalid from address: {from}"));
            }

            let wallet = self.wallet_client.as_ref()
                .ok_or_else(|| "WalletDb not initialized — sync required".to_string())?;

            // Parse account from derivation path (stored in Account)
            // For now, account 0 — the from address is a UA, not a derivation path
            // The actual account index should be looked up from the account DB
            let account = 0; // TODO: look up from address/account DB

            // Parse amount from human-readable string to zatoshis
            let amount_f64: f64 = amount.parse().map_err(|_| "invalid amount".to_string())?;
            let amount_zat = (amount_f64 * 100_000_000.0) as u64;

            // The public_key is the Orchard FVK bytes stored in Account::viewing_key
            // We need to pass it through. For now, the from address is a UA.
            // In practice, the caller should look up the viewing key from the DB.
            let public_key = vec![]; // TODO: get from account viewing key

            wallet.create_transaction_async(
                public_key,
                account,
                to.clone(),
                amount_zat,
                memo.clone(),
            ).await
        }
        _ => Err("unexpected intent variant for Zcash protocol".to_string()),
    }
}
```

3. Implement `get_balance()`:
```rust
async fn get_balance(&self, _address: &str, _asset: &str) -> Result<paypunk_types::Balance, String> {
    let wallet = self.wallet_client.as_ref()
        .ok_or_else(|| "WalletDb not initialized — sync required".to_string())?;

    // Query the WalletDb for note balances
    // This requires sending a message to WalletDbActor
    // For now, we delegate via the wallet_client
    // The actual implementation queries zcash_client_sqlite for spendable/pending notes

    // TODO: Implement actual balance query via WalletDbActor
    // This will require a new WalletMessage variant for balance queries
    Err("get_balance via WalletDb not yet implemented".to_string())
}
```

4. Implement `broadcast()`:
```rust
async fn broadcast(&self, finalized_tx: &[u8]) -> Result<String, String> {
    let host = self.lightwalletd_host.as_ref()
        .ok_or_else(|| "lightwalletd not configured".to_string())?;

    let lsp = crate::lsp_client::LspClient::connect(host, self.params).await?;
    lsp.broadcast_tx(finalized_tx).await
}
```

## Verification
- `cargo build -p paypunk-chains-zcash` succeeds
