# Step 7: Create RealWalletApi + wire real send flow

## Description

Create `RealWalletApi` that wraps `paypunk_api::Client` and implements the `WalletApi` trait. Wire the Ethereum send two-phase flow: `submit_intent()` → preview → `approve_signature()` → `broadcast_transaction()`. Wire `RealWalletApi` into the TUI when a socket path is provided.

## Files to create

- `tui/src/api/real.rs` — `RealWalletApi` struct and `#[async_trait] impl WalletApi`

## Files to modify

- `tui/src/api/mod.rs` — Add `pub mod real;`
- `tui/src/lib.rs` — Create `RealWalletApi` when socket path is provided
- `tui/src/screens/send.rs` — Wire real two-phase flow in `handle_input` (use `submit_intent` result for review data, `approve_signature` + `broadcast` for confirm)
- `cli/src/main.rs` — Pass `--socket-path` to `paypunk_tui::run_tui()` in the `Tui` subcommand

## Acceptance Criteria

- [ ] `cargo build` succeeds with no warnings
- [ ] `cargo test` passes all tests
- [ ] `RealWalletApi` connects to paypunkd via `api::Client::connect()`
- [ ] `RealWalletApi::submit_send_review` calls `submit_intent()` and returns `SendReviewData` parsed from `ArtifactSummary`
- [ ] `RealWalletApi::submit_send_confirm` calls `approve_signature()` then `broadcast_transaction()` and returns `SendResult`
- [ ] The TUI launches with `--socket-path` and uses the real backend
- [ ] The TUI launches without `--socket-path` and uses the mock backend

## Detailed Steps

1. Create `tui/src/api/real.rs`:
   ```rust
   use async_trait::async_trait;
   use paypunk_api::Client;
   use paypunk_types::{ArtifactSummary, EthereumIntent, Intent, ProtocolId};
   use std::sync::Mutex;
   use zeroize::Zeroizing;

   use super::types::*;
   use super::WalletApi;

   /// Internal state held between submit_send_review and submit_send_confirm.
   struct PendingSend {
       raw_artifact: Vec<u8>,
       keypunkd_signature: Vec<u8>,
       keypunkd_public_key: [u8; 32],
       derivation_path: Vec<u8>,
   }

   pub struct RealWalletApi {
       client: Client,
       pending: Mutex<Option<PendingSend>>,
   }

   impl RealWalletApi {
       pub async fn connect(socket_path: &str) -> Result<Self, String> {
           let client = Client::connect(socket_path).await?;
           Ok(Self {
               client,
               pending: Mutex::new(None),
           })
       }

       pub fn with_client(client: Client) -> Self {
           Self {
               client,
               pending: Mutex::new(None),
           }
       }
   }

   #[async_trait]
   impl WalletApi for RealWalletApi {
       async fn get_setup(&self) -> SetupData {
           // For now, return basic setup data. In a real impl we'd check
           // if a seed exists via an API call.
           SetupData {
               app_version: "0.1.0".to_string(),
               wallet_exists: false,
               new_mnemonic: vec![],
               word_count: 12,
               import_methods: vec!["mnemonic".into()],
           }
       }

       async fn submit_setup_create(&self, input: SetupCreateInput) -> Result<(), ApiError> {
           self.client
               .generate_seed(Zeroizing::new(input.password))
               .await
               .map(|_| ())
               .map_err(|e| ApiError(e))
       }

       async fn submit_setup_import(&self, input: SetupImportInput) -> Result<(), ApiError> {
           self.client
               .restore_seed(Zeroizing::new(input.secret), Zeroizing::new(input.password))
               .await
               .map_err(|e| ApiError(e))
       }

       async fn get_wallets(&self) -> WalletsData {
           // Placeholder — real derivation requires password
           WalletsData { wallets: vec![] }
       }

       async fn get_assets(&self, chain_id: &str) -> AssetsData {
           // Placeholder
           if chain_id.contains("eip155") {
               AssetsData {
                   assets: vec![AssetRow {
                       name: "Ethereum".into(),
                       ticker: "ETH".into(),
                       price: "$2,000.00".into(),
                       price_change: "▲ 0.00%".into(),
                       price_change_up: true,
                       holdings_value: "$0.00".into(),
                       holdings_amount: "0 ETH".into(),
                       chain_id: chain_id.into(),
                   }],
               }
           } else {
               AssetsData { assets: vec![] }
           }
       }

       async fn get_home(&self) -> HomeData {
           // Placeholder
           HomeData {
               accounts: vec![],
               balances: vec![],
               total_fiat_value: 0.0,
               fiat_currency: "USD".into(),
               pending_tx: None,
           }
       }

       async fn submit_home(&self, _input: HomeInput) -> HomeData {
           self.get_home().await
       }

       async fn home_state(&self) -> ApiState<HomeData> {
           ApiState::Loaded(self.get_home().await)
       }

       async fn refresh_home(&self) {}

       async fn get_receive(&self, chain_id: &str) -> ReceiveData {
           // Placeholder — needs derive_address with password
           ReceiveData {
               address: "not_derived".into(),
               chain_id: chain_id.into(),
               address_format: "hex".into(),
               qr_payload: String::new(),
           }
       }

       async fn submit_receive(&self, _input: ReceiveInput) -> ReceiveData {
           self.get_receive("").await
       }

       async fn receive_state(&self, chain_id: &str) -> ApiState<ReceiveData> {
           ApiState::Loaded(self.get_receive(chain_id).await)
       }

       async fn refresh_receive(&self, _chain_id: &str) {}

       async fn get_send(&self, chain_id: &str) -> SendData {
           // Placeholder — needs derive_address with password + get_balance
           let is_eth = chain_id.contains("eip155");
           SendData {
               from_address: "0x0000000000000000000000000000000000000000".into(),
               spendable_balance: "0".into(),
               decimals: if is_eth { 18 } else { 8 },
               chain_id: chain_id.into(),
               fee_data: if is_eth {
                   FeeData::Eth(FeeDataEth {
                       base_fee_per_gas: "0".into(),
                       max_priority_fee_per_gas: "0".into(),
                       gas_limit_estimate: "21000".into(),
                   })
               } else {
                   FeeData::Zec(FeeRates { slow: 0, medium: 0, fast: 0 })
               },
               nonce: if is_eth { Some(0) } else { None },
               utxos: None,
           }
       }

       async fn submit_send_review(&self, input: SendReviewInput) -> SendReviewData {
           // Build intent
           let intent = Intent::Ethereum(EthereumIntent::Transfer {
               to: input.to_address.clone(),
               amount: input.amount.clone(),
               from: "0x0000000000000000000000000000000000000000".into(), // placeholder
               asset: input.token_id.clone(),
               data: None,
           });

           let path = 0u32.to_le_bytes();

           match self.client.submit_intent(intent, &path).await {
               Ok((raw_artifact, parsed_summary, keypunkd_signature, keypunkd_public_key)) => {
                   // Store pending state
                   let pending = PendingSend {
                       raw_artifact,
                       keypunkd_signature,
                       keypunkd_public_key,
                       derivation_path: path.to_vec(),
                   };
                   *self.pending.lock().unwrap() = Some(pending);

                   // Parse summary
                   if let Ok(summary) = postcard::from_bytes::<ArtifactSummary>(&parsed_summary) {
                       SendReviewData {
                           to_address: summary.to,
                           amount: summary.amount,
                           fee_estimate: summary.fee,
                           total_amount: summary.amount.clone(), // simplified
                           chain_id: input.chain_id,
                       }
                   } else {
                       SendReviewData {
                           to_address: input.to_address,
                           amount: input.amount,
                           fee_estimate: "unknown".into(),
                           total_amount: input.amount,
                           chain_id: input.chain_id,
                       }
                   }
               }
               Err(e) => {
                   // Return error info as review data (will show in UI)
                   SendReviewData {
                       to_address: format!("Error: {e}"),
                       amount: String::new(),
                       fee_estimate: String::new(),
                       total_amount: String::new(),
                       chain_id: input.chain_id,
                   }
               }
           }
       }

       async fn submit_send_confirm(&self, _input: SendConfirmInput) -> SendResult {
           let pending = self.pending.lock().unwrap().take();
           match pending {
               Some(p) => {
                   // Approve signature (placeholder — needs password from UI)
                   match self
                       .client
                       .approve_signature(
                           &p.raw_artifact,
                           &p.keypunkd_signature,
                           Zeroizing::new("password".to_string()),
                           &p.derivation_path,
                       )
                       .await
                   {
                       Ok(signed_artifact) => {
                           match self
                               .client
                               .broadcast_transaction(ProtocolId::Ethereum, signed_artifact)
                               .await
                           {
                               Ok(tx_hash) => SendResult {
                                   tx_hash: tx_hash.clone(),
                                   status: "broadcasted".into(),
                                   block_explorer_url: format!(
                                       "https://etherscan.io/tx/{}",
                                       tx_hash
                                   ),
                               },
                               Err(e) => SendResult {
                                   tx_hash: String::new(),
                                   status: format!("broadcast failed: {e}"),
                                   block_explorer_url: String::new(),
                               },
                           }
                       }
                       Err(e) => SendResult {
                           tx_hash: String::new(),
                           status: format!("signing failed: {e}"),
                           block_explorer_url: String::new(),
                       },
                   }
               }
               None => SendResult {
                   tx_hash: String::new(),
                   status: "error: no pending transaction".into(),
                   block_explorer_url: String::new(),
               },
           }
       }

       async fn send_state(&self, chain_id: &str) -> ApiState<SendData> {
           ApiState::Loaded(self.get_send(chain_id).await)
       }

       async fn refresh_send(&self, _chain_id: &str) {}

       async fn get_lock(&self) -> LockData {
           LockData {
               auth_methods: LockAuthMethods {
                   biometric_available: false,
                   password_set: true,
               },
               failed_attempts: 0,
           }
       }

       async fn submit_lock(&self, _input: LockInput) -> Result<(), ApiError> {
           Ok(())
       }

       async fn get_settings(&self) -> SettingsData {
           SettingsData {
               security: SecuritySettings {
                   biometric_enabled: false,
                   auto_lock_minutes: 5,
               },
               fiat_currency: "USD".into(),
               app_version: "0.1.0".into(),
           }
       }

       async fn submit_settings(&self, _input: SettingsInput) -> Result<(), ApiError> {
           Ok(())
       }

       async fn submit_reveal_phrase(
           &self,
           _input: RevealPhraseInput,
       ) -> Result<Vec<String>, ApiError> {
           Err(ApiError("reveal phrase not yet supported via real API".into()))
       }
   }
   ```

2. Open `tui/src/api/mod.rs`. Add `pub mod real;`.

3. Open `tui/src/lib.rs`. Update the `run_tui` function to create `RealWalletApi` when a socket path is provided:
   ```rust
   pub async fn run_tui(socket_path: Option<String>) -> io::Result<()> {
       let api: Box<dyn WalletApi> = if let Some(path) = socket_path {
           match RealWalletApi::connect(&path).await {
               Ok(real) => Box::new(real),
               Err(e) => {
                   eprintln!("Failed to connect to paypunkd at {path}: {e}");
                   eprintln!("Falling back to mock API");
                   Box::new(MockWalletApi::new())
               }
           }
       } else {
           Box::new(MockWalletApi::new())
       };
       // ... rest of the function
   }
   ```
   Add the import: `use crate::api::real::RealWalletApi;`

4. Open `tui/src/screens/send.rs`. Update the `handle_input` method — it already calls `api.submit_send_review()` and `api.submit_send_confirm()`, which are now async. The existing flow (Form → Review → ConfirmSend → Sending → Confirm) maps well to the real API. The main change is that the methods are now `await`ed. Update the `handle_input` signature:
   ```rust
   async fn handle_input(&mut self, key: KeyEvent, api: &mut dyn WalletApi) -> Nav {
   ```
   And add `.await` to the API calls:
   ```rust
   let review = api.submit_send_review(...).await;
   // ...
   let result = api.submit_send_confirm(...).await;
   ```

5. Open `cli/src/main.rs`. Update the `Tui` command handler to pass the socket path:
   ```rust
   None | Some(Commands::Tui) => {
       let socket_path = cli.socket_path;
       paypunk_tui::run_tui(Some(socket_path)).await?;
       Ok(())
   }
   ```
   Note: `run_tui` is now async, so the `Tui` path needs to run inside the tokio runtime. The CLI's `async_main` already runs in a tokio runtime, so we can call it from there.

6. Run `cargo build` and fix any compilation errors.

7. Run `cargo test` and verify all tests pass.

## Completion

When this step is complete — **STOP. DO NOT START THE NEXT STEP.**

```bash
git add -A && git commit -m "step 7: create RealWalletApi + wire real send flow"

mv todo/07_step.md done/

cat >> todo/goal.md << 'EOF'

## Step 7 — Done

Created `RealWalletApi` in `tui/src/api/real.rs` wrapping `api::Client`. Implemented the two-phase send flow (submit_intent → approve_signature → broadcast). Wired real vs mock selection via `--socket-path`. Updated CLI to pass socket path to TUI.
EOF
```

## ⛔ STOP HERE. WAIT FOR INSTRUCTION. DO NOT READ OR EXECUTE STEP 8.
