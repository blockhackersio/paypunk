use paypunk_types::{Protocol, ProtocolId, SignerProtocol, WalletRepository};
use std::str::FromStr;

use crate::address;

pub struct EthereumProtocol;

impl Protocol for EthereumProtocol {
    fn protocol_id(&self) -> ProtocolId {
        ProtocolId::Ethereum
    }

    fn derive_address(&self, public_key: &[u8], index: u32) -> Result<String, String> {
        let _ = index; // Ethereum uses BIP44 path; index is embedded in observation key derivation
        address::derive_from_pubkey(public_key)
            .map_err(|e| e.to_string())
    }

    fn propose_and_build(
        &self,
        _public_key: &[u8],
        _repository: &dyn WalletRepository,
        _account: u32,
        _to: &str,
        _amount: u64,
        _memo: Option<&str>,
    ) -> Result<Vec<u8>, String> {
        Err("Ethereum propose_and_build not yet implemented".to_string())
    }

    fn prove_transaction(&self, transaction: &[u8]) -> Result<Vec<u8>, String> {
        // Ethereum has no ZK proofs — this is a no-op.
        Ok(transaction.to_vec())
    }

    fn finalize_transaction(&self, transaction: &[u8]) -> Result<Vec<u8>, String> {
        // Ethereum: the signed transaction is returned as-is
        Ok(transaction.to_vec())
    }
}

impl SignerProtocol for EthereumProtocol {
    fn protocol_id(&self) -> ProtocolId {
        ProtocolId::Ethereum
    }

    fn derive_public_key(&self, seed: &[u8; 64], account: u32) -> Result<Vec<u8>, String> {
        let path = format!("m/44'/60'/{account}'/0/0");
        let parsed = bip32::DerivationPath::from_str(&path)
            .map_err(|e| format!("invalid path: {e}"))?;
        let key = bip32::ExtendedPrivateKey::<k256::ecdsa::SigningKey>::derive_from_path(*seed, &parsed)
            .map_err(|e| format!("BIP32 derivation failed: {e}"))?;
        let ext_pubkey = key.public_key();
        let inner = ext_pubkey.public_key();
        let point = inner.to_encoded_point(false);
        Ok(point.as_bytes().to_vec())
    }

    fn sign_transaction(
        &self,
        _seed: &[u8; 64],
        _account: u32,
        _transaction: &[u8],
    ) -> Result<Vec<u8>, String> {
        Err("Ethereum signing not yet implemented".to_string())
    }
}
