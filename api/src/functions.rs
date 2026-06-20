use keypunkd::crypto::Keypair;
use paypunk_types::{Account, AssetId, Balance, Intent, ProtocolId};
use zeroize::Zeroizing;
use argon2::Argon2;

fn hash_for_domain(password: &str, domain: &[u8]) -> Zeroizing<String> {
    let mut hash = [0u8; 32];
    Argon2::default()
        .hash_password_into(password.as_bytes(), domain, &mut hash)
        .expect("Argon2id key derivation should not fail with valid parameters");
    Zeroizing::new(hex::encode(hash))
}

/// Check whether a wallet seed exists on keypunkd.
pub async fn check_wallet_exists(
    service: &paypunkd::services::PaypunkService,
) -> Result<bool, String> {
    service.has_seed().await
}

/// Unlock the wallet by decrypting the DB and deriving initial accounts.
///
/// 1. Creates ephemeral keypair
/// 2. Fetches keypunkd's public key from paypunkd
/// 3. Fetches paypunkd's public encryption key
/// 4. Encrypts password to paypunkd's key (for DB unlock)
/// 5. Encrypts password to keypunkd's key (for bulk derivation)
/// 6. Sends Unlock to paypunkd with both encrypted payloads
/// 7. Returns accounts count from UnlockSuccess
pub async fn unlock(
    service: &paypunkd::services::PaypunkService,
    password: Zeroizing<String>,
) -> Result<u32, String> {
    let client_keypair = Keypair::new();
    let keypunk_pk = service.get_keypunk_encryption_key().await?;
    let paypunkd_pk = service.get_paypunkd_encryption_key().await?;
    let client_pk = client_keypair.public_key();

    let encrypted_keypunkd_password =
        client_keypair.encrypt(hash_for_domain(&password, b"keypunkd-seed-key"), &keypunk_pk);
    let encrypted_db_password =
        client_keypair.encrypt(hash_for_domain(&password, b"paypunkd-db-key"), &paypunkd_pk);

    service
        .unlock(
            encrypted_db_password,
            client_pk,
            encrypted_keypunkd_password,
            client_pk,
        )
        .await
}

/// Generate a new wallet seed.
pub async fn generate_seed(
    service: &paypunkd::services::PaypunkService,
    password: Zeroizing<String>,
) -> Result<Zeroizing<String>, String> {
    let client_keypair = Keypair::new();
    let server_pk = service.get_keypunk_encryption_key().await?;
    let encrypted_password =
        client_keypair.encrypt(hash_for_domain(&password, b"keypunkd-seed-key"), &server_pk);
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
    let encrypted_password =
        client_keypair.encrypt(hash_for_domain(&password, b"keypunkd-seed-key"), &server_pk);
    let client_pk = client_keypair.public_key();

    service
        .restore_seed(encrypted_mnemonic, encrypted_password, client_pk)
        .await
}

/// Derive an address for the given protocol, CAIP-10 account, and index.
pub async fn derive_address(
    service: &paypunkd::services::PaypunkService,
    password: Zeroizing<String>,
    protocol: ProtocolId,
    account: String,
    index: u32,
) -> Result<String, String> {
    let client_keypair = Keypair::new();
    let server_pk = service.get_keypunk_encryption_key().await?;
    let encrypted_password =
        client_keypair.encrypt(hash_for_domain(&password, b"keypunkd-seed-key"), &server_pk);
    let client_pk = client_keypair.public_key();

    service
        .derive_address(encrypted_password, client_pk, protocol, account, index)
        .await
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

    let hashed_password = hash_for_domain(&password, b"keypunkd-seed-key");
    // Encode payload: raw_len(4) + raw + sig_len(4) + sig + hashed_pw
    let mut payload = Vec::new();
    payload.extend_from_slice(&(raw_artifact.len() as u32).to_le_bytes());
    payload.extend_from_slice(raw_artifact);
    payload.extend_from_slice(&(keypunkd_signature.len() as u32).to_le_bytes());
    payload.extend_from_slice(keypunkd_signature);
    payload.extend_from_slice(hashed_password.as_bytes());

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

/// Broadcast a finalized, signed transaction to the network.
pub async fn broadcast_transaction(
    service: &paypunkd::services::PaypunkService,
    protocol: ProtocolId,
    raw_tx: Vec<u8>,
) -> Result<String, String> {
    service.broadcast_transaction(protocol, raw_tx).await
}

/// Create a new account from a pre-derived viewing key (no password needed).
pub async fn create_account(
    service: &paypunkd::services::PaypunkService,
    protocol: ProtocolId,
    derivation_path: String,
    account_index: u32,
    name: String,
) -> Result<Account, String> {
    service
        .create_account(
            protocol,
            derivation_path,
            account_index,
            name,
        )
        .await
}

/// List all accounts from the database.
pub async fn list_accounts(
    service: &paypunkd::services::PaypunkService,
) -> Result<Vec<Account>, String> {
    service.list_accounts().await
}

/// Get a single account by ID.
pub async fn get_account(
    service: &paypunkd::services::PaypunkService,
    id: String,
) -> Result<Option<Account>, String> {
    service.get_account(id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_for_domain_returns_hex_string() {
        let hash = hash_for_domain("mypassword", b"test-domain");
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_hash_for_domain_deterministic() {
        let a = hash_for_domain("password", b"test-domain-long");
        let b = hash_for_domain("password", b"test-domain-long");
        assert_eq!(*a, *b);
    }

    #[test]
    fn test_hash_for_domain_different_domains() {
        let a = hash_for_domain("password", b"domain-a-long-1");
        let b = hash_for_domain("password", b"domain-b-long-2");
        assert_ne!(*a, *b);
    }

    #[test]
    fn test_hash_for_domain_different_passwords() {
        let a = hash_for_domain("password-one", b"test-domain-long");
        let b = hash_for_domain("password-two", b"test-domain-long");
        assert_ne!(*a, *b);
    }
}
