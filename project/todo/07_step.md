# Step 7: Update RealWalletApi with real backend calls

## Context

The `RealWalletApi` currently returns hardcoded stub data for most methods. This step wires it to the real backend for all Ethereum operations.

## Changes

### `tui/src/api/real.rs`

**Remove `PendingSend` struct** — keep as-is (it stores the pending artifact for two-phase auth).

**Remove `derivation_index` field** — derive account index from selected account.

**Update `get_home()`:**
```rust
async fn get_home(&self) -> HomeData {
    match self.client.list_accounts().await {
        Ok(accounts) => {
            let account_rows: Vec<AccountInfo> = accounts.iter().map(|a| {
                let chain_id = match a.protocol {
                    ProtocolId::Ethereum => "eip155:1".to_string(),
                    _ => format!("{}:0", format!("{:?}", a.protocol).to_lowercase()),
                };
                AccountInfo {
                    account_id: a.id.clone(),
                    name: a.name.clone(),
                    address: a.address.clone(),
                    chain_id,
                    protocol: format!("{:?}", a.protocol),
                }
            }).collect();
            HomeData {
                accounts: account_rows,
                fiat_currency: "USD".into(),
            }
        }
        Err(e) => HomeData { accounts: vec![], fiat_currency: "USD".into() },
    }
}
```

**Add `list_accounts()`:**
```rust
async fn list_accounts(&self) -> Result<Vec<AccountInfo>, ApiError> {
    let accounts = self.client.list_accounts().await.map_err(ApiError)?;
    Ok(accounts.iter().map(|a| AccountInfo {
        account_id: a.id.clone(),
        name: a.name.clone(),
        address: a.address.clone(),
        chain_id: "eip155:1".to_string(),
        protocol: format!("{:?}", a.protocol),
    }).collect())
}
```

**Add `add_account()`:**
```rust
async fn add_account(&self) -> Result<(), ApiError> {
    let accounts = self.client.list_accounts().await.map_err(ApiError)?;
    let eth_accounts: Vec<_> = accounts.iter().filter(|a| a.protocol == ProtocolId::Ethereum).collect();
    let next_index = eth_accounts.len() as u32;
    // Parse account index from derivation path "m/44'/60'/{index}'"
    let _ = self.client.create_account(
        ProtocolId::Ethereum,
        format!("m/44'/60'/{next_index}'"),
        next_index,
        format!("Ethereum Account {next_index}"),
    ).await.map_err(ApiError)?;
    Ok(())
}
```

**Update `get_send(account_id)`:**
```rust
async fn get_send(&self, account_id: &str) -> SendData {
    match self.client.get_account(account_id.to_string()).await {
        Ok(Some(account)) => {
            let caip10 = format!("eip155:1:{}", account.address);
            let balance = self.client.get_balance(caip10, "eip155:1/slip44:60".to_string()).await
                .map(|b| b.spendable.0.to_string())
                .unwrap_or_else(|_| "0".to_string());
            SendData {
                account_id: account_id.to_string(),
                from_address: account.address,
                spendable_balance: balance,
                decimals: 18,
                chain_id: "eip155:1".to_string(),
            }
        }
        _ => SendData {
            account_id: account_id.to_string(),
            from_address: String::new(),
            spendable_balance: "0".to_string(),
            decimals: 18,
            chain_id: "eip155:1".to_string(),
        },
    }
}
```

**Update `get_receive(account_id)`:**
```rust
async fn get_receive(&self, account_id: &str) -> ReceiveData {
    match self.client.get_account(account_id.to_string()).await {
        Ok(Some(account)) => ReceiveData {
            address: account.address.clone(),
            chain_id: "eip155:1".to_string(),
            address_format: "hex".to_string(),
            qr_payload: account.address,
            account_id: account_id.to_string(),
        },
        _ => ReceiveData {
            address: "unknown".into(),
            chain_id: "eip155:1".into(),
            address_format: "hex".into(),
            qr_payload: String::new(),
            account_id: account_id.to_string(),
        },
    }
}
```

**Update `get_assets(account_id)`:**
```rust
async fn get_assets(&self, account_id: &str) -> AssetsData {
    match self.client.get_account(account_id.to_string()).await {
        Ok(Some(account)) => {
            let caip10 = format!("eip155:1:{}", account.address);
            let balance = self.client.get_balance(caip10, "eip155:1/slip44:60".to_string()).await
                .map(|b| b.spendable.0.to_string())
                .unwrap_or_else(|_| "0".to_string());
            AssetsData {
                assets: vec![AssetRow {
                    name: "Ethereum".into(),
                    ticker: "ETH".into(),
                    price: "—".into(),
                    price_change: "—".into(),
                    price_change_up: true,
                    holdings_value: "—".into(),
                    holdings_amount: balance,
                    chain_id: "eip155:1".into(),
                }],
            }
        }
        _ => AssetsData { assets: vec![] },
    }
}
```

**Update `submit_send_review()`:**
- Look up the account by `input.account_id` to get the real `from` address and `derivation_path`
- The derivation path for signing is the account's `derivation_path` field (e.g., `"m/44'/60'/0'"`). Convert it to bytes by extracting the account index (the last number before `'`):
  ```rust
  // Parse account index from derivation path like "m/44'/60'/0'"
  fn parse_account_index(path: &str) -> u32 {
      path.rsplit('\'').nth(1)
          .and_then(|s| s.split('/').last())
          .and_then(|s| s.parse().ok())
          .unwrap_or(0)
  }
  let account_index = parse_account_index(&account.derivation_path);
  let path_bytes = account_index.to_le_bytes();
  ```
- Construct `Intent::Ethereum(Transfer { from: account.address, to: input.to_address, amount: input.amount, asset: "eip155:1/slip44:60", data: None })`
- Call `self.client.submit_intent(intent, &path_bytes)`
```rust
if let Ok(summary) = postcard::from_bytes::<ArtifactSummary>(&parsed_summary) {
    SendReviewData {
        to_address: summary.to,
        amount: summary.amount.clone(),
        fee_estimate: summary.fee,
        total_amount: summary.amount,
        chain_id: input.chain_id,
        nonce: summary.nonce,
    }
}
```

**Update `submit_send_confirm()`:**
- Read password from `input.auth_confirmation.value`
- Call `approve_signature` then `broadcast_transaction` as before

**Update `home_state()`, `refresh_home()`, `send_state()`, `refresh_send()`, `receive_state()`, `refresh_receive()`:**
- Change parameter from `chain_id` to `account_id`
- Use account_id for caching keys

## Acceptance Criteria

- [ ] `get_home()` returns real accounts from backend
- [ ] `get_send(account_id)` returns real from_address and balance
- [ ] `get_receive(account_id)` returns real address
- [ ] `get_assets(account_id)` returns real ETH balance
- [ ] `add_account()` creates a new account in the backend
- [ ] `submit_send_review()` constructs intents with real from_address
- [ ] `submit_send_confirm()` uses password from input
- [ ] `cargo build` succeeds
