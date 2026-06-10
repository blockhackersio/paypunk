use alloy_consensus::{SignableTransaction, TxEip1559};
use alloy_primitives::{Address, Signature, TxKind, U256};
use k256::ecdsa::signature::hazmat::PrehashSigner;
use k256::ecdsa::{RecoveryId, SigningKey, VerifyingKey};
use paypunk_types::{Protocol, ProtocolId, SignerProtocol};
use std::str::FromStr;

use crate::address;

pub struct EthereumProtocol;

impl EthereumProtocol {
    pub const COIN_TYPE: u32 = 60;
    pub const CHAIN_ID: u64 = 1;
}

impl Protocol for EthereumProtocol {
    fn protocol_id(&self) -> ProtocolId {
        ProtocolId::Ethereum
    }

    fn derive_address(&self, public_key: &[u8], _index: u32) -> Result<String, String> {
        let addr = address::derive_from_pubkey(public_key).map_err(|e| e.to_string())?;
        Ok(addr.to_string())
    }

    fn validate_address(&self, address: &str) -> bool {
        address::validate_address(address)
    }

    fn finalize_transaction(&self, transaction: &[u8]) -> Result<Vec<u8>, String> {
        let mut buf = transaction;
        let signed = alloy_consensus::Signed::<TxEip1559>::eip2718_decode(&mut buf)
            .map_err(|e| format!("invalid tx: {e}"))?;
        let mut out = Vec::new();
        signed.eip2718_encode(&mut out);
        Ok(out)
    }

    fn create_transaction(
        &self,
        _public_key: &[u8],
        _account: u32,
        to: &str,
        amount: u64,
        memo: Option<&str>,
    ) -> Result<Vec<u8>, String> {
        let to_addr: Address = to.parse().map_err(|e| format!("invalid address: {e}"))?;

        let input = memo
            .map(|m| alloy_primitives::Bytes::from(m.as_bytes().to_vec()))
            .unwrap_or_default();

        let tx = TxEip1559 {
            chain_id: Self::CHAIN_ID,
            nonce: 0,
            gas_limit: 21_000,
            max_fee_per_gas: 20_000_000_000,
            max_priority_fee_per_gas: 1_000_000_000,
            to: TxKind::Call(to_addr),
            value: U256::from(amount),
            input,
            access_list: Default::default(),
        };

        Ok(alloy_rlp::encode(&tx))
    }

    fn get_balance(&self, _account: u32) -> Result<paypunk_types::Balance, String> {
        Err("get_balance not yet implemented — needs RPC endpoint".to_string())
    }
}

impl SignerProtocol for EthereumProtocol {
    fn protocol_id(&self) -> ProtocolId {
        ProtocolId::Ethereum
    }

    fn derive_public_key(&self, seed: &[u8; 64], account: u32) -> Result<Vec<u8>, String> {
        let path = format!("m/44'/60'/{account}'/0/0");
        let parsed =
            bip32::DerivationPath::from_str(&path).map_err(|e| format!("invalid path: {e}"))?;
        let key = bip32::ExtendedPrivateKey::<SigningKey>::derive_from_path(*seed, &parsed)
            .map_err(|e| format!("BIP32 derivation failed: {e}"))?;
        let ext_pubkey = key.public_key();
        let inner = ext_pubkey.public_key();
        let point = inner.to_encoded_point(false);
        Ok(point.as_bytes().to_vec())
    }

    fn sign_transaction(
        &self,
        seed: &[u8; 64],
        account: u32,
        transaction: &[u8],
    ) -> Result<Vec<u8>, String> {
        let path = format!("m/44'/60'/{account}'/0/0");
        let parsed =
            bip32::DerivationPath::from_str(&path).map_err(|e| format!("invalid path: {e}"))?;
        let key = bip32::ExtendedPrivateKey::<SigningKey>::derive_from_path(*seed, &parsed)
            .map_err(|e| format!("BIP32 derivation failed: {e}"))?;
        let sk = key.private_key();

        let tx: TxEip1559 =
            alloy_rlp::decode_exact(transaction).map_err(|e| format!("RLP decode failed: {e}"))?;

        let sighash = tx.signature_hash();

        let sig = sk
            .sign_prehash(sighash.as_ref())
            .map_err(|e| format!("signing failed: {e}"))?;

        let rec_id = [0u8, 1]
            .into_iter()
            .find_map(|id| {
                let rid = RecoveryId::from_byte(id)?;
                VerifyingKey::recover_from_prehash(sighash.as_ref(), &sig, rid)
                    .ok()
                    .filter(|recovered| recovered == sk.verifying_key())
                    .map(|_| rid)
            })
            .ok_or_else(|| "could not determine recovery ID".to_string())?;

        let alloy_sig = Signature::new(
            U256::from_be_slice(&sig.r().to_bytes()),
            U256::from_be_slice(&sig.s().to_bytes()),
            rec_id.is_y_odd(),
        );

        let signed = tx.into_signed(alloy_sig);
        let mut out = Vec::new();
        signed.eip2718_encode(&mut out);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bip39::Mnemonic;

    const MNEMONIC: &str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    fn seed_from_mnemonic() -> [u8; 64] {
        let mnemonic: Mnemonic = MNEMONIC.parse().unwrap();
        mnemonic.to_seed("")
    }

    #[test]
    fn test_derive_public_key() {
        let protocol = EthereumProtocol;
        let seed = seed_from_mnemonic();
        let pk = protocol.derive_public_key(&seed, 0).unwrap();
        assert_eq!(pk.len(), 65);
        assert_eq!(pk[0], 0x04);
    }

    #[test]
    fn test_derive_address_roundtrip() {
        let protocol = EthereumProtocol;
        let seed = seed_from_mnemonic();
        let pk = protocol.derive_public_key(&seed, 0).unwrap();
        let addr = protocol.derive_address(&pk, 0).unwrap();
        assert!(addr.starts_with("0x"));
        assert_eq!(addr.len(), 42);
    }

    #[test]
    fn test_create_and_sign_transaction() {
        let protocol = EthereumProtocol;
        let seed = seed_from_mnemonic();
        let pk = protocol.derive_public_key(&seed, 0).unwrap();

        let unsigned = protocol
            .create_transaction(
                &pk,
                0,
                "0xd8da6bf26964af9d7eed9e03e53415d37aa96045",
                100_000,
                None,
            )
            .unwrap();
        assert!(!unsigned.is_empty());

        let signed = protocol.sign_transaction(&seed, 0, &unsigned).unwrap();
        assert!(!signed.is_empty());

        let finalized = protocol.finalize_transaction(&signed).unwrap();
        assert_eq!(signed, finalized);
    }

    #[test]
    fn test_validate_address() {
        let protocol = EthereumProtocol;
        assert!(protocol.validate_address("0x9858effd232b4033e47d90003d41ec34ecaeda94"));
        assert!(!protocol.validate_address("invalid"));
    }
}
