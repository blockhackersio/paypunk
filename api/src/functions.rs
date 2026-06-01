use keypunkd::crypto::Keypair;
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

    let encrypted_mnemonic = service
        .generate_seed(encrypted_password, client_pk)
        .await?;

    let mnemonic = client_keypair
        .decrypt(&encrypted_mnemonic, &server_pk)
        .map_err(|e| e.to_string())?;
    Ok(mnemonic)
}
