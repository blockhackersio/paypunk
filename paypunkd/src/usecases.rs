use keypunkd::services::KeypunkService;

pub async fn get_keypunk_public_key(
    service: &KeypunkService,
) -> Result<[u8; 32], String> {
    service.get_public_key().await
}
