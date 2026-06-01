use paypunk_ipc::IpcSender;
use paypunkd::services::PaypunkService;
use tactix::Sender;
use zeroize::Zeroizing;

/// High-level wallet client that hides IPC and service details.
pub struct Client {
    service: PaypunkService,
}

impl Client {
    /// Connect to a running `paypunkd` instance over its Unix socket.
    pub async fn connect(socket_path: &str) -> Result<Self, String> {
        let ipc = IpcSender::connect(socket_path)
            .await
            .map_err(|e| e.to_string())?;
        let service = PaypunkService::new(ipc.recipient());
        Ok(Self { service })
    }

    /// Generate a new wallet seed, encrypt it with the given password,
    /// and return the 12-word BIP39 mnemonic.
    pub async fn generate_seed(
        &self,
        password: Zeroizing<String>,
    ) -> Result<Zeroizing<String>, String> {
        crate::functions::generate_seed(&self.service, password).await
    }
}
