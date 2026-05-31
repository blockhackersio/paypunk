use keypunkd::crypto::{ClientCrypto, KeyStore};
use keypunkd::dispatcher::Dispatcher;
use keypunkd::messages::{KeypunkdRequest, KeypunkdResponse};
use keypunkd::seed_store::InMemorySeedStore;
use paypunk_ipc::IpcMessage;
use tactix::{Actor, Sender};

#[tokio::test]
async fn test_get_public_key() {
    let keystore = KeyStore::new();
    let store = InMemorySeedStore::new();
    let addr = Dispatcher::new(keystore, store).start();

    let bytes = postcard::to_allocvec(&KeypunkdRequest::GetPublicKey).unwrap();
    let response_bytes = addr.ask(IpcMessage(bytes)).await.unwrap();
    let response: KeypunkdResponse = postcard::from_bytes(&response_bytes).unwrap();

    match response {
        KeypunkdResponse::PublicKey { key } => {
            assert_eq!(key.len(), 32);
        }
        other => panic!("expected PublicKey, got {other:?}"),
    }
}

#[tokio::test]
async fn test_generate_seed_no_filesystem() {
    let keystore = KeyStore::new();
    let store = InMemorySeedStore::new();
    let addr = Dispatcher::new(keystore, store).start();

    // Client side
    let server_pk = {
        let bytes = postcard::to_allocvec(&KeypunkdRequest::GetPublicKey).unwrap();
        let response_bytes = addr.ask(IpcMessage(bytes)).await.unwrap();
        let response: KeypunkdResponse = postcard::from_bytes(&response_bytes).unwrap();
        match response {
            KeypunkdResponse::PublicKey { key } => key,
            _ => panic!("expected PublicKey"),
        }
    };

    let client = ClientCrypto::new();
    let encrypted_password = client.wrap_password("hunter2", &server_pk);
    let client_pk = client.public_key();

    let request = KeypunkdRequest::GenerateSeed {
        encrypted_password,
        client_public_key: client_pk,
    };
    let bytes = postcard::to_allocvec(&request).unwrap();
    let response_bytes = addr.ask(IpcMessage(bytes)).await.unwrap();
    let response: KeypunkdResponse = postcard::from_bytes(&response_bytes).unwrap();

    match response {
        KeypunkdResponse::SeedGenerated { encrypted_mnemonic } => {
            let mnemonic = client
                .unwrap_mnemonic(&encrypted_mnemonic, &server_pk)
                .unwrap();
            assert_eq!(mnemonic.split_whitespace().count(), 12);
        }
        other => panic!("expected SeedGenerated, got {other:?}"),
    }
}

#[tokio::test]
async fn test_generate_seed_empty_password() {
    let keystore = KeyStore::new();
    let store = InMemorySeedStore::new();
    let addr = Dispatcher::new(keystore, store).start();

    let server_pk = {
        let bytes = postcard::to_allocvec(&KeypunkdRequest::GetPublicKey).unwrap();
        let response_bytes = addr.ask(IpcMessage(bytes)).await.unwrap();
        let response: KeypunkdResponse = postcard::from_bytes(&response_bytes).unwrap();
        match response {
            KeypunkdResponse::PublicKey { key } => key,
            _ => panic!("expected PublicKey"),
        }
    };

    let client = ClientCrypto::new();
    let encrypted_password = client.wrap_password("", &server_pk);

    let request = KeypunkdRequest::GenerateSeed {
        encrypted_password,
        client_public_key: client.public_key(),
    };
    let bytes = postcard::to_allocvec(&request).unwrap();
    let response_bytes = addr.ask(IpcMessage(bytes)).await.unwrap();
    let response: KeypunkdResponse = postcard::from_bytes(&response_bytes).unwrap();

    match response {
        KeypunkdResponse::SeedGenerated { encrypted_mnemonic } => {
            let mnemonic = client
                .unwrap_mnemonic(&encrypted_mnemonic, &server_pk)
                .unwrap();
            assert_eq!(mnemonic.split_whitespace().count(), 12);
        }
        other => panic!("expected SeedGenerated, got {other:?}"),
    }
}
