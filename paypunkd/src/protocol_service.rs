use paypunk_chains_ethereum::protocol::EthereumProtocol;
use paypunk_chains_ethereum::rpc::EthRpcClient;
use paypunk_chains_zcash::protocol::ZcashProtocol;
use paypunk_types::{Protocol, ProtocolId};
use std::collections::HashMap;

/// A registry of non-signer protocol implementations.
///
/// Protocols are registered at startup in `main.rs` and never change
/// during the lifetime of the daemon. Adding a new protocol means
/// implementing `Protocol` in the chain crate and registering it here.
pub struct ProtocolService {
    protocols: HashMap<ProtocolId, Box<dyn Protocol>>,
}

impl ProtocolService {
    pub fn new() -> Self {
        Self {
            protocols: HashMap::new(),
        }
    }

    pub fn with_ethereum<T: EthRpcClient + 'static>(
        zcash: ZcashProtocol,
        ethereum: EthereumProtocol<T>,
    ) -> Self {
        let mut s = Self::new();
        s.register(Box::new(zcash));
        s.register(Box::new(ethereum));
        s
    }

    pub fn register(&mut self, protocol: Box<dyn Protocol>) {
        self.protocols.insert(protocol.protocol_id(), protocol);
    }

    pub fn get(&self, id: ProtocolId) -> Result<&dyn Protocol, String> {
        self.protocols
            .get(&id)
            .map(|b| b.as_ref())
            .ok_or_else(|| format!("unsupported protocol: {id:?}"))
    }

    pub fn protocols(&self) -> Vec<ProtocolId> {
        self.protocols.keys().copied().collect()
    }
}
