use keypunkd::crypto::Keypair;
use keypunkd::seed_store::InMemorySeedStore;
use keypunkd::Keypunkd;
use paypunk_api::Client;
use paypunk_ipc::IpcMessage;
use paypunkd::Paypunkd;
use tactix::{Actor, Recipient, Sender};
use zeroize::Zeroizing;

/// Wire up the full actor chain (no sockets) and drive it through the
/// public `paypunk_api::Client`.
fn wire_actors() -> Recipient<IpcMessage> {
    let keystore = Keypair::new();
    let store = InMemorySeedStore::new();
    let keypunkd_addr = Keypunkd::new(keystore, store)
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
