use keypunkd::crypto::Keypair;
use keypunkd::protocol::ProtocolService as KeypunkdProtocolService;
use keypunkd::seed_store::InMemorySeedStore;
use keypunkd::Keypunkd;
use paypunk_api::Client;
use paypunk_chains_ethereum::protocol::EthereumProtocol;
use paypunk_chains_ethereum::rpc::EthRpcClient;
use paypunk_chains_zcash::protocol::ZcashProtocol;
use paypunk_ipc::IpcMessage;
use paypunk_types::ProtocolId;
use paypunkd::protocol_service::ProtocolService;
use paypunkd::Paypunkd;
use tactix::{Actor, Recipient, Sender};
use zeroize::Zeroizing;

/// A mock RPC client that returns fixed balances for testing.
struct MockRpcClient {
    eth_balance: u64,
    erc20_balance: u64,
}

impl MockRpcClient {
    fn new(eth_balance: u64, erc20_balance: u64) -> Self {
        Self {
            eth_balance,
            erc20_balance,
        }
    }
}

impl EthRpcClient for MockRpcClient {
    fn get_eth_balance(&self, _address: &str) -> Result<u64, String> {
        Ok(self.eth_balance)
    }

    fn get_erc20_balance(&self, _address: &str, _token_address: &str) -> Result<u64, String> {
        Ok(self.erc20_balance)
    }
}

/// Wire up the full actor chain (no sockets) and drive it through the
/// public `paypunk_api::Client`.
fn wire_actors() -> Recipient<IpcMessage> {
    let keystore = Keypair::new();
    let store = InMemorySeedStore::new();

    // keypunkd uses SignerProtocol registry (still object-safe)
    let mut keypunkd_protocols = KeypunkdProtocolService::new();
    keypunkd_protocols.register(Box::new(ZcashProtocol {
        params: zcash_protocol::consensus::Network::MainNetwork,
    }));

    let keypunkd_addr = Keypunkd::new(keystore, store, keypunkd_protocols)
        .with_skip_session_auth(true)
        .start();
    let keypunkd_recipient = keypunkd_addr.recipient();

    let paypunkd_zcash = ZcashProtocol {
        params: zcash_protocol::consensus::Network::MainNetwork,
    };
    let paypunkd_ethereum = EthereumProtocol::new(MockRpcClient::new(0, 0));
    let paypunkd_protocols = ProtocolService::with_ethereum(paypunkd_zcash, paypunkd_ethereum);

    let paypunkd_addr = Paypunkd::new(keypunkd_recipient, paypunkd_protocols).start();
    paypunkd_addr.recipient()
}

/// Wire up actors with a custom Ethereum mock for balance tests.
fn wire_actors_with_eth_mock(mock: MockRpcClient) -> Recipient<IpcMessage> {
    let keystore = Keypair::new();
    let store = InMemorySeedStore::new();

    let mut keypunkd_protocols = KeypunkdProtocolService::new();
    keypunkd_protocols.register(Box::new(ZcashProtocol {
        params: zcash_protocol::consensus::Network::MainNetwork,
    }));
    // keypunkd also needs Ethereum for derive_public_key / sign
    keypunkd_protocols.register(Box::new(EthereumProtocol::new(())));

    let keypunkd_addr = Keypunkd::new(keystore, store, keypunkd_protocols)
        .with_skip_session_auth(true)
        .start();
    let keypunkd_recipient = keypunkd_addr.recipient();

    let paypunkd_zcash = ZcashProtocol {
        params: zcash_protocol::consensus::Network::MainNetwork,
    };
    let paypunkd_ethereum = EthereumProtocol::new(mock);
    let paypunkd_protocols = ProtocolService::with_ethereum(paypunkd_zcash, paypunkd_ethereum);

    let paypunkd_addr = Paypunkd::new(keypunkd_recipient, paypunkd_protocols).start();
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

    let recipient = wire_actors();
    let client = Client::with_recipient(recipient);

    let mnemonic_2 = client
        .generate_seed(Zeroizing::new("password2".to_string()))
        .await
        .unwrap();

    assert_ne!(mnemonic_1, mnemonic_2);
    assert_eq!(mnemonic_1.split_whitespace().count(), 12);
    assert_eq!(mnemonic_2.split_whitespace().count(), 12);
}

#[tokio::test]
async fn test_restore_seed_via_api() {
    let password = Zeroizing::new("hunter2".to_string());
    let recipient = wire_actors();
    let client = Client::with_recipient(recipient);

    let mnemonic = client.generate_seed(password.clone()).await.unwrap();
    assert_eq!(mnemonic.split_whitespace().count(), 12);

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

#[tokio::test]
async fn test_unlock_without_seed_fails() {
    let recipient = wire_actors();
    let client = Client::with_recipient(recipient);

    let result = client.unlock(Zeroizing::new("password".to_string())).await;

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

    let address = client
        .derive_address(ProtocolId::Zcash, 0, 0)
        .await
        .unwrap();
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

    let addr0 = client
        .derive_address(ProtocolId::Zcash, 0, 0)
        .await
        .unwrap();
    let addr1 = client
        .derive_address(ProtocolId::Zcash, 0, 1)
        .await
        .unwrap();
    let addr2 = client
        .derive_address(ProtocolId::Zcash, 0, 2)
        .await
        .unwrap();

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

    client.unlock(password.clone()).await.unwrap();
    let addr_a = client
        .derive_address(ProtocolId::Zcash, 0, 0)
        .await
        .unwrap();
    client.lock().await.unwrap();

    client.unlock(password).await.unwrap();
    let addr_b = client
        .derive_address(ProtocolId::Zcash, 0, 0)
        .await
        .unwrap();

    assert_eq!(
        addr_a, addr_b,
        "same seed + index must produce same address"
    );
}

#[tokio::test]
async fn test_lock_clears_session() {
    let recipient = wire_actors();
    let client = Client::with_recipient(recipient);

    let password = Zeroizing::new("password".to_string());
    client.generate_seed(password.clone()).await.unwrap();
    client.unlock(password).await.unwrap();

    client
        .derive_address(ProtocolId::Zcash, 0, 0)
        .await
        .unwrap();

    client.lock().await.unwrap();

    let addr = client
        .derive_address(ProtocolId::Zcash, 0, 0)
        .await
        .unwrap();
    assert!(addr.starts_with("u1"), "got: {addr}");
}

#[tokio::test]
async fn test_eth_balance_via_mock_rpc() {
    let mock = MockRpcClient::new(10_000_000_000_000_000_000, 5_000_000_000_000_000_000);
    let recipient = wire_actors_with_eth_mock(mock);
    let client = Client::with_recipient(recipient);

    let password = Zeroizing::new("hunter2".to_string());
    client.generate_seed(password.clone()).await.unwrap();
    client.unlock(password).await.unwrap();

    let balance = client.get_balance(ProtocolId::Ethereum, 0).await.unwrap();

    // 10 ETH in wei
    assert_eq!(balance.spendable.0, 10_000_000_000_000_000_000);
    assert_eq!(balance.total.0, 10_000_000_000_000_000_000);
    assert_eq!(balance.pending.0, 0);
}

#[tokio::test]
async fn test_eth_balance_zero() {
    let mock = MockRpcClient::new(0, 0);
    let recipient = wire_actors_with_eth_mock(mock);
    let client = Client::with_recipient(recipient);

    let password = Zeroizing::new("hunter2".to_string());
    client.generate_seed(password.clone()).await.unwrap();
    client.unlock(password).await.unwrap();

    let balance = client.get_balance(ProtocolId::Ethereum, 0).await.unwrap();

    assert_eq!(balance.spendable.0, 0);
    assert_eq!(balance.total.0, 0);
    assert_eq!(balance.pending.0, 0);
}
