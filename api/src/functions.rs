use keypunkd::crypto::Keypair;
use paypunk_types::{AssetId, Balance, Intent, ProtocolId};
use zeroize::Zeroizing;

/// Generate a new wallet seed.
pub async fn generate_seed(
    service: &paypunkd::services::PaypunkService,
    password: Zeroizing<String>,
) -> Result<Zeroizing<String>, String> {
    let client_keypair = Keypair::new();
    let server_pk = service.get_keypunk_encryption_key().await?;
    let encrypted_password = client_keypair.encrypt(password, &server_pk);
    let client_pk = client_keypair.public_key();

    let encrypted_mnemonic = service.generate_seed(encrypted_password, client_pk).await?;

    let mnemonic = client_keypair
        .decrypt(&encrypted_mnemonic, &server_pk)
        .map_err(|e| e.to_string())?;
    Ok(mnemonic)
}

/// Restore a wallet from an existing BIP39 mnemonic seed phrase.
pub async fn restore_seed(
    service: &paypunkd::services::PaypunkService,
    mnemonic: Zeroizing<String>,
    password: Zeroizing<String>,
) -> Result<(), String> {
    let client_keypair = Keypair::new();
    let server_pk = service.get_keypunk_encryption_key().await?;
    let encrypted_mnemonic = client_keypair.encrypt(mnemonic, &server_pk);
    let encrypted_password = client_keypair.encrypt(password, &server_pk);
    let client_pk = client_keypair.public_key();

    service
        .restore_seed(encrypted_mnemonic, encrypted_password, client_pk)
        .await
}

/// Unlock the wallet by sending the password to keypunkd.
pub async fn unlock(
    service: &paypunkd::services::PaypunkService,
    password: Zeroizing<String>,
) -> Result<(), String> {
    let client_keypair = Keypair::new();
    let server_pk = service.get_keypunk_encryption_key().await?;
    let encrypted_password = client_keypair.encrypt(password, &server_pk);
    let client_pk = client_keypair.public_key();

    service.unlock(encrypted_password, client_pk).await
}

/// Lock the wallet, zeroizing the in-memory seed in keypunkd.
pub async fn lock(service: &paypunkd::services::PaypunkService) -> Result<(), String> {
    service.lock().await
}

/// Derive an address for the given protocol, CAIP-10 account, and index.
pub async fn derive_address(
    service: &paypunkd::services::PaypunkService,
    protocol: ProtocolId,
    account: String,
    index: u32,
) -> Result<String, String> {
    service.derive_address(protocol, account, index).await
}

/// Submit an intent for preview.
///
/// Returns the raw artifact, parsed summary, keypunkd's signature over
/// H(raw, parsed, path), and keypunkd's public key for verification.
pub async fn submit_intent(
    service: &paypunkd::services::PaypunkService,
    intent: Intent,
    derivation_path: &[u8],
) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>, [u8; 32]), String> {
    match service
        .submit_intent(intent, derivation_path.to_vec())
        .await?
    {
        paypunkd::messages::PaypunkdResponse::SignablePreview {
            raw_artifact,
            parsed_summary,
            keypunkd_signature,
            keypunkd_public_key,
        } => Ok((
            raw_artifact,
            parsed_summary,
            keypunkd_signature,
            keypunkd_public_key,
        )),
        paypunkd::messages::PaypunkdResponse::Error { message } => Err(message),
        _ => Err("unexpected response from paypunkd".to_string()),
    }
}

/// Approve a previously previewed artifact by encrypting the password
/// along with the artifact and signature to keypunkd.
pub async fn approve_signature(
    service: &paypunkd::services::PaypunkService,
    raw_artifact: &[u8],
    keypunkd_signature: &[u8],
    password: Zeroizing<String>,
    derivation_path: &[u8],
) -> Result<Vec<u8>, String> {
    let client_keypair = Keypair::new();
    let server_pk = service.get_keypunk_encryption_key().await?;
    let client_pk = client_keypair.public_key();

    // Encode payload: raw_len(4) + raw + sig_len(4) + sig + pw
    let mut payload = Vec::new();
    payload.extend_from_slice(&(raw_artifact.len() as u32).to_le_bytes());
    payload.extend_from_slice(raw_artifact);
    payload.extend_from_slice(&(keypunkd_signature.len() as u32).to_le_bytes());
    payload.extend_from_slice(keypunkd_signature);
    payload.extend_from_slice(password.as_bytes());

    let encrypted_payload = client_keypair.encrypt_bytes(&payload, &server_pk);

    service
        .approve_signature(encrypted_payload, client_pk, derivation_path.to_vec())
        .await
}

/// Query the balance for the given address and asset (CAIP-10 and CAIP-19).
pub async fn get_balance(
    service: &paypunkd::services::PaypunkService,
    address: String,
    asset: String,
) -> Result<Balance, String> {
    service.get_balance(address, asset).await
}

/// Legacy balance query using protocol + account + AssetId.
pub async fn get_balance_legacy(
    service: &paypunkd::services::PaypunkService,
    protocol: ProtocolId,
    account: u32,
    asset: AssetId,
) -> Result<Balance, String> {
    // Convert to CAIP format
    let address = match protocol {
        ProtocolId::Ethereum => format!("eip155:1:{account}"),
        ProtocolId::Zcash => format!("zcash:mainnet:{account}"),
        _ => return Err("unsupported protocol".to_string()),
    };
    let asset_str = match asset {
        AssetId::Native => match protocol {
            ProtocolId::Ethereum => "eip155:1/slip44:60".to_string(),
            ProtocolId::Zcash => "zcash:mainnet/slip44:133".to_string(),
            _ => return Err("unsupported protocol".to_string()),
        },
        AssetId::Token(addr) => addr,
    };
    get_balance(service, address, asset_str).await
}
