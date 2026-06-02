use keypunkd::crypto::Keypair;
use paypunk_types::ProtocolId;
use zeroize::Zeroizing;

/// Generate a new wallet seed.
///
/// Creates an ephemeral X25519 keypair, encrypts the password to keypunkd's
/// public key, sends the request through paypunkd, and decrypts the returned
/// mnemonic.
pub async fn generate_seed(
    service: &paypunkd::services::PaypunkService,
    password: Zeroizing<String>,
) -> Result<Zeroizing<String>, String> {
    let client_keypair = Keypair::new();
    let server_pk = service.get_keypunk_public_key().await?;
    let encrypted_password = client_keypair.encrypt(password, &server_pk);
    let client_pk = client_keypair.public_key();

    let encrypted_mnemonic = service.generate_seed(encrypted_password, client_pk).await?;

    let mnemonic = client_keypair
        .decrypt(&encrypted_mnemonic, &server_pk)
        .map_err(|e| e.to_string())?;
    Ok(mnemonic)
}

/// Restore a wallet from an existing BIP39 mnemonic seed phrase.
///
/// Encrypts both the mnemonic and password to keypunkd's public key, sends
/// the request through paypunkd for validation and persistence.
pub async fn restore_seed(
    service: &paypunkd::services::PaypunkService,
    mnemonic: Zeroizing<String>,
    password: Zeroizing<String>,
) -> Result<(), String> {
    let client_keypair = Keypair::new();
    let server_pk = service.get_keypunk_public_key().await?;
    let encrypted_mnemonic = client_keypair.encrypt(mnemonic, &server_pk);
    let encrypted_password = client_keypair.encrypt(password, &server_pk);
    let client_pk = client_keypair.public_key();

    service
        .restore_seed(encrypted_mnemonic, encrypted_password, client_pk)
        .await
}

/// Unlock the wallet by sending the password to keypunkd.
///
/// keypunkd decrypts the seed and holds it in memory for subsequent
/// operations like address derivation.
pub async fn unlock(
    service: &paypunkd::services::PaypunkService,
    password: Zeroizing<String>,
) -> Result<(), String> {
    let client_keypair = Keypair::new();
    let server_pk = service.get_keypunk_public_key().await?;
    let encrypted_password = client_keypair.encrypt(password, &server_pk);
    let client_pk = client_keypair.public_key();

    service.unlock(encrypted_password, client_pk).await
}

/// Derive an address for the given protocol, account, and diversifier index.
///
/// paypunkd caches the protocol's view key material locally, so subsequent
/// calls for the same (protocol, account) do not require keypunkd roundtrips.
pub async fn derive_address(
    service: &paypunkd::services::PaypunkService,
    protocol: ProtocolId,
    account: u32,
    index: u32,
) -> Result<String, String> {
    service.derive_address(protocol, account, index).await
}

/// Sign a payload using the derived private key for the given protocol and account.
///
/// The signing request is forwarded to keypunkd where the private key material
/// lives in protected memory. The signature bytes are returned.
pub async fn sign(
    service: &paypunkd::services::PaypunkService,
    protocol: ProtocolId,
    account: u32,
    payload: Vec<u8>,
) -> Result<Vec<u8>, String> {
    service.sign(protocol, account, payload).await
}

/// Lock the wallet, zeroizing the in-memory seed in keypunkd.
pub async fn lock(service: &paypunkd::services::PaypunkService) -> Result<(), String> {
    service.lock().await
}
