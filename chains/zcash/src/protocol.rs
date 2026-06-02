use orchard::keys::{FullViewingKey, SpendingKey};
use paypunk_types::{Protocol, ProtocolId};
use zip32::AccountId;

pub struct ZcashProtocol;

impl Protocol for ZcashProtocol {
    fn protocol_id(&self) -> ProtocolId {
        ProtocolId::Zcash
    }

    fn derive_view_key(&self, seed: &[u8; 64], account: u32) -> Result<Vec<u8>, String> {
        let account_id =
            AccountId::try_from(account).map_err(|_| format!("invalid account: {account}"))?;
        let sk = SpendingKey::from_zip32_seed(seed, 133, account_id)
            .map_err(|e| format!("ZIP 32 derivation failed: {e}"))?;
        let fvk = FullViewingKey::from(&sk);
        Ok(fvk.to_bytes().to_vec())
    }

    fn sign(&self, _seed: &[u8; 64], _account: u32, _message: &[u8]) -> Result<Vec<u8>, String> {
        Err("Orchard signing not yet implemented".to_string())
    }
}
