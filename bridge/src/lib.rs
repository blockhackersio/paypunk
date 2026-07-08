pub struct BridgeConfig {
    pub port: u16,
    pub socket_path: String,
}

pub async fn run(_config: BridgeConfig) -> Result<(), Box<dyn std::error::Error>> {
    // Stub — will be implemented in step 2
    Ok(())
}
