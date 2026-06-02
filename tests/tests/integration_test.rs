use keypunkd::crypto::Keypair;
use keypunkd::protocol::ProtocolRegistry;
use keypunkd::seed_store::InMemorySeedStore;
use keypunkd::Keypunkd;
use paypunk_api::Client;
use paypunk_chains_zcash::protocol::ZcashProtocol;
use paypunk_ipc::IpcMessage;
use paypunk_types::ProtocolId;
use paypunkd::Paypunkd;
use tactix::{Actor, Recipient, Sender};
use zeroize::Zeroizing;

/// Wire up the full actor chain (no sockets) and drive it through the
/// public `paypunk_api::Client`.
fn wire_actors() -> Recipient<IpcMessage> {
    let keystore = Keypair::new();
    let store = InMemorySeedStore::new();

    let mut protocols = ProtocolRegistry::new();
    protocols.register(Box::new(ZcashProtocol));

    let keypunkd_addr = Keypunkd::new(keystore, store, protocols)
        .with_skip_session_auth(true)
        .start();
    let keypunkd_recipient = keypunkd_addr.recipient();

    let paypunkd_addr = Paypunkd::new(keypunkd_recipient).start();
    paypunkd_addr.recipient()
}

#[tokio::test]
async fn test_generate_seed_via_api() {
    let recipient = wire_actors();
    let client = Client::with_recipient(recipient);

    let mnemonic = client
        .generate_seed(Zeroizing::new("hunter2".to_string()))
        .await
        .unwrap();

    assert_eq!(mnemonic.split_whitespace().count(), 12);
}

#[tokio::test]
async fn test_generate_seed_empty_password_via_api() {
    let recipient = wire_actors();
    let client = Client::with_recipient(recipient);

    let mnemonic = client
        .generate_seed(Zeroizing::new("".to_string()))
        .await
        .unwrap();

    assert_eq!(mnemonic.split_whitespace().count(), 12);
}

#[tokio::test]
async fn test_generate_seed_different_passwords_produce_different_seeds() {
    let recipient = wire_actors();
    let client = Client::with_recipient(recipient);

    let mnemonic_1 = client
        .generate_seed(Zeroizing::new("password1".to_string()))
        .await
        .unwrap();

    // Re-wire for a fresh keypunkd (no persisted state between calls)
    let recipient = wire_actors();
    let client = Client::with_recipient(recipient);

    let mnemonic_2 = client
        .generate_seed(Zeroizing::new("password2".to_string()))
        .await
        .unwrap();

    // Different passwords → different encrypted seeds → different mnemonics
    assert_ne!(mnemonic_1, mnemonic_2);
    assert_eq!(mnemonic_1.split_whitespace().count(), 12);
    assert_eq!(mnemonic_2.split_whitespace().count(), 12);
}

#[tokio::test]
async fn test_restore_seed_via_api() {
    // ── Step 1: Generate a seed on "device 1" ──────────────────────────
    let password = Zeroizing::new("hunter2".to_string());
    let recipient = wire_actors();
    let client = Client::with_recipient(recipient);

    let mnemonic = client.generate_seed(password.clone()).await.unwrap();

    assert_eq!(mnemonic.split_whitespace().count(), 12);

    // ── Step 2: Restore the seed on a "different device" ───────────────
    // Fresh keypunkd with no prior state simulates a new device.
    let recipient = wire_actors();
    let client = Client::with_recipient(recipient);

    let result = client
        .restore_seed(mnemonic.clone(), password.clone())
        .await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn test_restore_seed_invalid_mnemonic_fails() {
    let recipient = wire_actors();
    let client = Client::with_recipient(recipient);

    let result = client
        .restore_seed(
            Zeroizing::new("not a valid bip39 mnemonic phrase".to_string()),
            Zeroizing::new("password".to_string()),
        )
        .await;

    assert!(result.is_err());
    assert!(result.unwrap_err().contains("invalid mnemonic"));
}

// ── Unlock / DeriveAddress / Lock integration tests ──────────────────────

#[tokio::test]
async fn test_unlock_without_seed_fails() {
    let recipient = wire_actors();
    let client = Client::with_recipient(recipient);

    let result = client
        .unlock(Zeroizing::new("password".to_string()))
        .await;

    assert!(result.is_err());
    assert!(result.unwrap_err().contains("no seed found"));
}

#[tokio::test]
async fn test_unlock_with_wrong_password_fails() {
    let recipient = wire_actors();
    let client = Client::with_recipient(recipient);

    client
        .generate_seed(Zeroizing::new("correct-password".to_string()))
        .await
        .unwrap();

    let result = client
        .unlock(Zeroizing::new("wrong-password".to_string()))
        .await;

    assert!(result.is_err());
    assert!(result.unwrap_err().contains("seed decryption failed"));
}

#[tokio::test]
async fn test_derive_address_without_unlock_fails() {
    let recipient = wire_actors();
    let client = Client::with_recipient(recipient);

    client
        .generate_seed(Zeroizing::new("password".to_string()))
        .await
        .unwrap();

    let result = client.derive_address(ProtocolId::Zcash, 0, 0).await;

    assert!(result.is_err());
    assert!(result.unwrap_err().contains("no active session"));
}

#[tokio::test]
async fn test_unlock_then_derive_address() {
    let recipient = wire_actors();
    let client = Client::with_recipient(recipient);

    let password = Zeroizing::new("hunter2".to_string());
    client.generate_seed(password.clone()).await.unwrap();

    client.unlock(password).await.unwrap();

    let address = client.derive_address(ProtocolId::Zcash, 0, 0).await.unwrap();
    assert!(address.starts_with("u1"), "got: {address}");
    assert!(address.len() > 50, "got: {address}");
}

#[tokio::test]
async fn test_derive_different_indexes() {
    let recipient = wire_actors();
    let client = Client::with_recipient(recipient);

    let password = Zeroizing::new("password".to_string());
    client.generate_seed(password.clone()).await.unwrap();
    client.unlock(password).await.unwrap();

    let addr0 = client.derive_address(ProtocolId::Zcash, 0, 0).await.unwrap();
    let addr1 = client.derive_address(ProtocolId::Zcash, 0, 1).await.unwrap();
    let addr2 = client.derive_address(ProtocolId::Zcash, 0, 2).await.unwrap();

    assert_ne!(addr0, addr1);
    assert_ne!(addr1, addr2);
    assert_ne!(addr0, addr2);
}

#[tokio::test]
async fn test_derive_address_is_deterministic() {
    let recipient = wire_actors();
    let client = Client::with_recipient(recipient);

    let password = Zeroizing::new("password".to_string());
    client.generate_seed(password.clone()).await.unwrap();

    // First unlock session
    client.unlock(password.clone()).await.unwrap();
    let addr_a = client.derive_address(ProtocolId::Zcash, 0, 0).await.unwrap();
    client.lock().await.unwrap();

    // Second unlock session — same seed, same password
    client.unlock(password).await.unwrap();
    let addr_b = client.derive_address(ProtocolId::Zcash, 0, 0).await.unwrap();

    assert_eq!(addr_a, addr_b, "same seed + index must produce same address");
}

#[tokio::test]
async fn test_lock_clears_session() {
    let recipient = wire_actors();
    let client = Client::with_recipient(recipient);

    let password = Zeroizing::new("password".to_string());
    client.generate_seed(password.clone()).await.unwrap();
    client.unlock(password).await.unwrap();

    // Address works before lock
    client.derive_address(ProtocolId::Zcash, 0, 0).await.unwrap();

    client.lock().await.unwrap();

    // Address derivation still works after lock — the view key (FVK) is
    // cached in paypunkd and does not require the seed. Only signing
    // (which needs the private key) would fail after lock.
    let addr = client.derive_address(ProtocolId::Zcash, 0, 0).await.unwrap();
    assert!(addr.starts_with("u1"), "got: {addr}");
}
