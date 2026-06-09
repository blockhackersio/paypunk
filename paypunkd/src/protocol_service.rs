use paypunk_chains_ethereum::protocol::EthereumProtocol;
use paypunk_chains_zcash::protocol::ZcashProtocol;
use paypunk_types::{Protocol, ProtocolId};

/// Concrete protocol service — no `dyn` dispatch.
/// Holds one instance per supported chain and dispatches via match.
pub struct ProtocolService {
    pub zcash: ZcashProtocol,
    pub ethereum: EthereumProtocol,
}

impl ProtocolService {
    pub fn new(zcash: ZcashProtocol, ethereum: EthereumProtocol) -> Self {
        Self { zcash, ethereum }
    }

    pub fn derive_address(
        &self,
        id: ProtocolId,
        public_key: &[u8],
        index: u32,
    ) -> Result<String, String> {
        match id {
            ProtocolId::Zcash => self.zcash.derive_address(public_key, index),
            ProtocolId::Ethereum => self.ethereum.derive_address(public_key, index),
            _ => Err(format!("unsupported protocol: {id:?}")),
        }
    }

    pub fn prove_transaction(&self, id: ProtocolId, transaction: &[u8]) -> Result<Vec<u8>, String> {
        match id {
            ProtocolId::Zcash => self.zcash.prove_transaction(transaction),
            ProtocolId::Ethereum => self.ethereum.prove_transaction(transaction),
            _ => Err(format!("unsupported protocol: {id:?}")),
        }
    }

    pub fn finalize_transaction(
        &self,
        id: ProtocolId,
        transaction: &[u8],
    ) -> Result<Vec<u8>, String> {
        match id {
            ProtocolId::Zcash => self.zcash.finalize_transaction(transaction),
            ProtocolId::Ethereum => self.ethereum.finalize_transaction(transaction),
            _ => Err(format!("unsupported protocol: {id:?}")),
        }
    }
}
