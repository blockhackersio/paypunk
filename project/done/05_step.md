# Step 5: Wire reveal phrase through the full IPC chain

## Issue
**#4** — `submit_reveal_phrase()` in `tui/src/api/real.rs:541` returns `Err("reveal phrase not yet supported via real API")`. The full IPC chain through paypunkd → keypunkd → seed.enc decrypt → mnemonic export is not wired.

The keypunkd already has `seed_store.rs` with seed decryption logic. The `ExportViewingKey` path shows the pattern for encrypted IPC with keypunkd.

## What to do

1. **Add IPC message in keypunkd** (`keypunkd/src/messages.rs`):
   - `ExportMnemonic { encrypted_password: Vec<u8>, client_public_key: [u8; 32] }`
   - Response: `MnemonicExported { encrypted_mnemonic: Vec<u8> }` or `Error`

2. **Implement handler in keypunkd** (`keypunkd/src/keypunkd.rs` or `keypunkd/src/usecases.rs`):
   - Decrypt the password using the keypunkd keypair
   - Read and decrypt `seed.enc`
   - Return the mnemonic encrypted to the client's public key
   - This is similar to how `GenerateSeed` returns the encrypted mnemonic

3. **Add IPC message in paypunkd** (`paypunkd/src/messages.rs`):
   - `RevealPhrase { encrypted_password: Vec<u8>, client_public_key: [u8; 32] }`
   - Response: `PhraseRevealed { encrypted_mnemonic: Vec<u8> }` or `Error`

4. **Implement handler in paypunkd** (`paypunkd/src/paypunkd.rs`):
   - Forward to keypunkd's `ExportMnemonic`
   - Return the encrypted mnemonic to the client

5. **Wire RealWalletApi** (`tui/src/api/real.rs`):
   - In `submit_reveal_phrase()`, call IPC `RevealPhrase` with encrypted password
   - Decrypt the returned mnemonic using the client keypair
   - Split into words and return as `Vec<String>`

6. **Add IPC methods to `PaypunkService`** (`paypunkd/src/services.rs`) and **api `Client`** (`api/src/client.rs`, `api/src/functions.rs`).

## Verification
- `cargo build` succeeds
- `cargo test` passes
- `submit_reveal_phrase()` with correct password returns the 12-word mnemonic
- `submit_reveal_phrase()` with wrong password returns an error
- The mnemonic is never exposed in plaintext over IPC (encrypted end-to-end)
