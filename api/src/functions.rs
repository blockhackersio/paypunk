use zeroize::Zeroizing;

/// Generate a new wallet seed.
///
/// # TODO
/// - Implement actual seed generation via keypunkd's GenerateSeed message.
/// - The password should be encrypted to keypunkd's public key before sending.
/// - The returned mnemonic will come back encrypted and need decryption.
pub async fn generate_seed(
    service: &paypunkd::services::PaypunkService,
    _password: Zeroizing<String>,
) -> Result<Zeroizing<String>, String> {
    let public_key = service.get_keypunk_public_key().await?;
    println!(
        "keypunkd public key: {}",
        public_key.iter().map(|b| format!("{:02x}", b)).collect::<String>()
    );
    Ok(Zeroizing::new(String::new()))
}
