use std::collections::HashMap;
use paypunk_types::{Protocol, ProtocolId};

/// A hardcoded registry of protocol implementations.
///
/// Protocols are registered at startup in `main.rs` and never change
/// during the lifetime of the daemon. Adding a new protocol means
/// implementing `Protocol` in the chain crate and registering it here.
pub struct ProtocolRegistry {
    protocols: HashMap<ProtocolId, Box<dyn Protocol>>,
}

impl ProtocolRegistry {
    pub fn new() -> Self {
        Self {
            protocols: HashMap::new(),
        }
    }

    pub fn register(&mut self, protocol: Box<dyn Protocol>) {
        self.protocols.insert(protocol.protocol_id(), protocol);
    }

    pub fn get(&self, id: ProtocolId) -> Option<&dyn Protocol> {
        self.protocols.get(&id).map(|b| b.as_ref())
    }
}
