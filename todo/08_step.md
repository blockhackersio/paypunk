# Step 8: Fix Ethereum nonce + add end-to-end integration test

## Description

Fix the hardcoded `nonce = 0` in `EthereumProtocol::build()` to fetch the real on-chain nonce via `get_transaction_count()`. Add an end-to-end integration test that exercises the full Ethereum send flow: generate seed → derive address → submit_intent → approve_signature → broadcast.

## Files to modify

- `protocols/ethereum/src/protocol.rs` — Replace `let nonce = 0` with actual RPC call
- `tests/tests/integration_test.rs` — Add `test_eth_send_full_flow` test

## Acceptance Criteria

- [ ] `cargo build` succeeds with no warnings
- [ ] `cargo test` passes all tests, including the new end-to-end test
- [ ] `EthereumProtocol::build()` uses `get_transaction_count()` instead of hardcoded 0
- [ ] The new integration test validates the complete flow: submit_intent returns preview data, approve_signature returns signed tx, broadcast returns tx hash

## Detailed Steps

1. Open `protocols/ethereum/src/protocol.rs`. Find line 66 (`let nonce = 0;`). Replace with:
   ```rust
   let nonce = self.client.get_transaction_count(from)?;
   ```

2. Open `tests/tests/integration_test.rs`. Add a new test at the bottom:
   ```rust
   #[tokio::test]
   async fn test_eth_send_full_flow() {
       let recipient = TestBuilder::new()
           .with_eth_balance(100_000_000_000_000_000_000) // 100 ETH
           .build();
       let client = Client::with_recipient(recipient);

       let password = Zeroizing::new("hunter2".to_string());
       client.generate_seed(password.clone()).await.unwrap();

       // Derive the Ethereum address
       let addr = client
           .derive_address(password.clone(), ProtocolId::Ethereum, "eip155:1:0".to_string(), 0)
           .await
           .unwrap();

       // Phase 1: Submit intent
       let intent = Intent::Ethereum(EthereumIntent::Transfer {
           to: "0xd8da6bf26964af9d7eed9e03e53415d37aa96045".to_string(),
           amount: "0.0001".to_string(),
           from: addr.clone(),
           asset: "eip155:1/slip44:60".to_string(),
           data: None,
       });
       let path = 0u32.to_le_bytes();

       let (raw_artifact, parsed_summary, signature, keypunkd_pk) = client
           .submit_intent(intent, &path)
           .await
           .expect("submit_intent should succeed");

       assert!(!raw_artifact.is_empty(), "raw_artifact should not be empty");
       assert!(!parsed_summary.is_empty(), "parsed_summary should not be empty");
       assert!(!signature.is_empty(), "signature should not be empty");

       // Verify the parsed summary
       let summary: ArtifactSummary =
           postcard::from_bytes(&parsed_summary).expect("should deserialize ArtifactSummary");
       assert_eq!(summary.protocol, ProtocolId::Ethereum);
       assert_eq!(
           summary.to,
           "0xd8da6bf26964af9d7eed9e03e53415d37aa96045"
       );

       // Phase 2: Approve and sign
       let signed_artifact = client
           .approve_signature(&raw_artifact, &signature, password.clone(), &path)
           .await
           .expect("approve_signature should succeed");

       assert!(!signed_artifact.is_empty(), "signed_artifact should not be empty");

       // Phase 3: Broadcast
       let tx_hash = client
           .broadcast_transaction(ProtocolId::Ethereum, signed_artifact)
           .await
           .expect("broadcast should succeed");

       assert!(!tx_hash.is_empty(), "tx_hash should not be empty");
       assert_eq!(tx_hash, "0xdeadbeef", "should match mock RPC response");
   }
   ```

   Add the necessary imports at the top of the test file if not already present:
   ```rust
   use paypunk_types::{ArtifactSummary, EthereumIntent, Intent, ProtocolId};
   ```

3. Run `cargo build` and verify it compiles.

4. Run `cargo test` and verify all tests pass, especially the new `test_eth_send_full_flow`.

## Completion

When this step is complete — **STOP. THIS IS THE FINAL STEP.**

```bash
git add -A && git commit -m "step 8: fix Ethereum nonce + add end-to-end integration test"

mv todo/08_step.md done/

cat >> todo/goal.md << 'EOF'

## Step 8 — Done

Fixed `nonce = 0` → `get_transaction_count()` in Ethereum protocol. Added `test_eth_send_full_flow` integration test covering submit_intent → approve_signature → broadcast.
EOF

echo "=== All 8 steps complete ==="
echo "Goal: TUI → API Ethereum send is wired end-to-end."
echo ""
echo "To verify the TUI with real backend:"
echo "  1. Start keypunkd: cargo run --bin keypunkd"
echo "  2. Start paypunkd: cargo run --bin paypunkd"
echo "  3. Launch TUI:    cargo run --bin paypunk-tui -- --socket-path /tmp/paypunkd.sock"
echo ""
echo "Or use the CLI to test the send flow directly:"
echo "  cargo run --bin paypunk -- generate-seed --password hunter2"
echo "  cargo run --bin paypunk -- submit-eth-transfer --to 0x... --amount 0.0001 --from 0x..."
```

## ⛔ ALL DONE. NO MORE STEPS.
