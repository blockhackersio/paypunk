use paypunk_ipc::IpcMessage;
use paypunk_ipc::IpcSender;
use paypunkd::services::PaypunkService;
use tactix::{Recipient, Sender};
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

    /// Create a client from an existing IPC recipient, bypassing Unix sockets.
    /// Useful for testing where actors are wired directly in-process.
    pub fn with_recipient(recipient: Recipient<IpcMessage>) -> Self {
        Self {
            service: PaypunkService::new(recipient),
        }
    }

    /// Generate a new wallet seed, encrypt it with the given password,
    /// and return the 12-word BIP39 mnemonic.
    pub async fn generate_seed(
        &self,
        password: Zeroizing<String>,
    ) -> Result<Zeroizing<String>, String> {
        crate::functions::generate_seed(&self.service, password).await
    }

    /// Restore a wallet from an existing BIP39 mnemonic seed phrase and password.
    pub async fn restore_seed(
        &self,
        mnemonic: Zeroizing<String>,
        password: Zeroizing<String>,
    ) -> Result<(), String> {
        crate::functions::restore_seed(&self.service, mnemonic, password).await
    }
}
