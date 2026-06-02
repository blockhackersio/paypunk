use keypunkd::services::KeypunkService;

pub async fn get_keypunk_public_key(service: &KeypunkService) -> Result<[u8; 32], String> {
    service.get_public_key().await
}

pub async fn generate_seed(
    service: &KeypunkService,
    encrypted_password: Vec<u8>,
    client_public_key: [u8; 32],
) -> Result<Vec<u8>, String> {
    service
        .generate_seed(encrypted_password, client_public_key)
        .await
}

pub async fn restore_seed(
    service: &KeypunkService,
    encrypted_mnemonic: Vec<u8>,
    encrypted_password: Vec<u8>,
    client_public_key: [u8; 32],
) -> Result<(), String> {
    service
        .restore_seed(encrypted_mnemonic, encrypted_password, client_public_key)
        .await
}

pub async fn unlock(
    service: &KeypunkService,
    encrypted_password: Vec<u8>,
    client_public_key: [u8; 32],
) -> Result<(), String> {
    service
        .unlock(encrypted_password, client_public_key)
        .await
}

pub async fn derive_address(
    service: &KeypunkService,
    index: u32,
) -> Result<String, String> {
    service.derive_address(index).await
}

pub async fn lock(service: &KeypunkService) -> Result<(), String> {
    service.lock().await
}
