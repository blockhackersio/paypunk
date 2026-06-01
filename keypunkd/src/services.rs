use paypunk_ipc::IpcMessage;
use tactix::{Recipient, Sender};

pub struct KeypunkService {
    recipient: Recipient<IpcMessage>,
}

impl KeypunkService {
    pub fn new(recipient: Recipient<IpcMessage>) -> Self {
        Self { recipient }
    }

    pub async fn get_public_key(&self) -> Result<[u8; 32], String> {
        let request = crate::messages::KeypunkdRequest::GetPublicKey;
        let payload =
            postcard::to_allocvec(&request).map_err(|e| format!("serialize error: {e}"))?;
        let msg = IpcMessage {
            payload,
            sender_public_key: None,
        };
        let response_bytes = self.recipient.ask(msg).await?;
        let response: crate::messages::KeypunkdResponse =
            postcard::from_bytes(&response_bytes).map_err(|e| format!("deserialize error: {e}"))?;
        match response {
            crate::messages::KeypunkdResponse::PublicKey { key } => Ok(key),
            crate::messages::KeypunkdResponse::Error { message } => Err(message),
            _ => Err("unexpected response variant".to_string()),
        }
    }
}
