use std::collections::HashMap;
use paypunk_types::{ProtocolId, SignerProtocol};

/// A hardcoded registry of signer protocol implementations.
///
/// Protocols are registered at startup in `main.rs` and never change
/// during the lifetime of the daemon. Adding a new protocol means
/// implementing `SignerProtocol` in the chain crate and registering it here.
pub struct ProtocolRegistry {
    protocols: HashMap<ProtocolId, Box<dyn SignerProtocol>>,
}

impl ProtocolRegistry {
    pub fn new() -> Self {
        Self {
            protocols: HashMap::new(),
        }
    }

    pub fn register(&mut self, protocol: Box<dyn SignerProtocol>) {
        self.protocols.insert(protocol.protocol_id(), protocol);
    }

    pub fn get(&self, id: ProtocolId) -> Option<&dyn SignerProtocol> {
        self.protocols.get(&id).map(|b| b.as_ref())
    }
}
