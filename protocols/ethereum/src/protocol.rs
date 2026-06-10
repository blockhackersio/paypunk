use alloy_consensus::{SignableTransaction, TxEip1559};
use alloy_primitives::{Address, Signature, TxKind, U256};
use k256::ecdsa::signature::hazmat::PrehashSigner;
use k256::ecdsa::{RecoveryId, SigningKey, VerifyingKey};
use paypunk_types::{AssetId, Balance, Protocol, ProtocolId, SignerProtocol};
use std::str::FromStr;
use hex;

use crate::address;
use crate::rpc::EthRpcClient;

pub struct EthereumProtocol<T: EthRpcClient> {
    pub client: T,
}

impl<T: EthRpcClient> EthereumProtocol<T> {
    pub const COIN_TYPE: u32 = 60;

    pub fn new(client: T) -> Self {
        Self { client }
    }

    /// Derive the Ethereum address from a public key for use as the "from" field.
    fn address_from_pubkey(&self, public_key: &[u8]) -> Result<String, String> {
        let addr = address::derive_from_pubkey(public_key).map_err(|e| e.to_string())?;
        Ok(addr.to_string())
    }
}

impl<T: EthRpcClient> Protocol for EthereumProtocol<T> {
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
        public_key: &[u8],
        _account: u32,
        to: &str,
        amount: u64,
        asset: &AssetId,
        memo: Option<&str>,
    ) -> Result<Vec<u8>, String> {
        let from_addr = self.address_from_pubkey(public_key)?;
        let to_addr: Address = to.parse().map_err(|e| format!("invalid address: {e}"))?;

        let chain_id = self.client.get_chain_id()?;
        let nonce = self.client.get_transaction_count(&from_addr)?;
        let gas_price = self.client.get_gas_price()?;
        let priority_fee = 1_000_000_000; // 1 gwei tip

        let (value, tx_to, base_gas, input, data_str) = match asset {
            AssetId::Native => {
                let input = memo
                    .map(|m| alloy_primitives::Bytes::from(m.as_bytes().to_vec()))
                    .unwrap_or_default();
                let data_str = if input.is_empty() {
                    String::new()
                } else {
                    format!("0x{}", hex::encode(&input))
                };
                (U256::from(amount), TxKind::Call(to_addr), 21_000u64, input, data_str)
            }
            AssetId::Token(contract) => {
                let contract_addr: Address = contract
                    .parse()
                    .map_err(|e| format!("invalid token contract: {e}"))?;
                let input = encode_erc20_transfer(&to_addr, amount);
                let data_str = format!("0x{}", hex::encode(&input));
                (U256::ZERO, TxKind::Call(contract_addr), 65_000u64, input, data_str)
            }
        };

        let gas_limit = self.client.estimate_gas(&from_addr, to, &format!("0x{:x}", amount), &data_str)?;
        let gas_limit = gas_limit.max(base_gas);

        let tx = TxEip1559 {
            chain_id,
            nonce,
            gas_limit,
            max_fee_per_gas: gas_price,
            max_priority_fee_per_gas: priority_fee,
            to: tx_to,
            value,
            input,
            access_list: Default::default(),
        };

        Ok(alloy_rlp::encode(&tx))
    }

    fn get_balance(&self, _account: u32, public_key: &[u8], asset: &AssetId) -> Result<Balance, String> {
        let addr = self.address_from_pubkey(public_key)?;
        let balance = self.client.get_balance(&addr, asset)?;
        Ok(Balance {
            spendable: paypunk_types::Amount(balance),
            pending: paypunk_types::Amount(0),
            total: paypunk_types::Amount(balance),
        })
    }
}

/// Encode an ERC-20 `transfer(address,uint256)` call.
fn encode_erc20_transfer(recipient: &Address, amount: u64) -> alloy_primitives::Bytes {
    let mut data = Vec::with_capacity(68);
    // transfer(address,uint256) selector: 0xa9059cbb
    data.extend_from_slice(&[0xa9, 0x05, 0x9c, 0xbb]);
    // recipient (32 bytes, left-padded)
    let mut recipient_bytes = [0u8; 32];
    recipient_bytes[12..].copy_from_slice(recipient.as_ref());
    data.extend_from_slice(&recipient_bytes);
    // amount (32 bytes, left-padded)
    let amount_bytes = U256::from(amount).to_be_bytes::<32>();
    data.extend_from_slice(&amount_bytes);
    alloy_primitives::Bytes::from(data)
}

impl<T: EthRpcClient> SignerProtocol for EthereumProtocol<T> {
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
    use crate::rpc::EthRpcClient;
    use bip39::Mnemonic;

    const MNEMONIC: &str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    /// A mock RPC client that returns fixed balances for testing.
    struct MockRpcClient {
        eth_balance: u64,
        erc20_balance: u64,
    }

    impl MockRpcClient {
        fn new(eth_balance: u64, erc20_balance: u64) -> Self {
            Self {
                eth_balance,
                erc20_balance,
            }
        }
    }

    impl EthRpcClient for MockRpcClient {
        fn get_balance(&self, _address: &str, asset: &AssetId) -> Result<u64, String> {
            match asset {
                AssetId::Native => Ok(self.eth_balance),
                AssetId::Token(_) => Ok(self.erc20_balance),
            }
        }
        fn get_transaction_count(&self, _address: &str) -> Result<u64, String> {
            Ok(0)
        }
        fn get_chain_id(&self) -> Result<u64, String> {
            Ok(1)
        }
        fn send_raw_transaction(&self, _raw_tx: &[u8]) -> Result<String, String> {
            Ok("0xdeadbeef".to_string())
        }
        fn get_gas_price(&self) -> Result<u128, String> {
            Ok(20_000_000_000)
        }
        fn estimate_gas(&self, _from: &str, _to: &str, _value: &str, _data: &str) -> Result<u64, String> {
            Ok(21_000)
        }
        fn get_block_number(&self) -> Result<u64, String> {
            Ok(19_000_000)
        }
        fn get_transaction_receipt(&self, _tx_hash: &str) -> Result<Option<crate::rpc::TxReceipt>, String> {
            Ok(None)
        }
    }

    fn seed_from_mnemonic() -> [u8; 64] {
        let mnemonic: Mnemonic = MNEMONIC.parse().unwrap();
        mnemonic.to_seed("")
    }

    #[test]
    fn test_derive_public_key() {
        let protocol = EthereumProtocol::new(MockRpcClient::new(0, 0));
        let seed = seed_from_mnemonic();
        let pk = protocol.derive_public_key(&seed, 0).unwrap();
        assert_eq!(pk.len(), 65);
        assert_eq!(pk[0], 0x04);
    }

    #[test]
    fn test_derive_address_roundtrip() {
        let protocol = EthereumProtocol::new(MockRpcClient::new(0, 0));
        let seed = seed_from_mnemonic();
        let pk = protocol.derive_public_key(&seed, 0).unwrap();
        let addr = protocol.derive_address(&pk, 0).unwrap();
        assert!(addr.starts_with("0x"));
        assert_eq!(addr.len(), 42);
    }

    #[test]
    fn test_create_and_sign_transaction() {
        let protocol = EthereumProtocol::new(MockRpcClient::new(0, 0));
        let seed = seed_from_mnemonic();
        let pk = protocol.derive_public_key(&seed, 0).unwrap();

        let unsigned = protocol
            .create_transaction(
                &pk,
                0,
                "0xd8da6bf26964af9d7eed9e03e53415d37aa96045",
                100_000,
                &AssetId::Native,
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
    fn test_create_erc20_transaction() {
        let protocol = EthereumProtocol::new(MockRpcClient::new(0, 0));
        let seed = seed_from_mnemonic();
        let pk = protocol.derive_public_key(&seed, 0).unwrap();

        let unsigned = protocol
            .create_transaction(
                &pk,
                0,
                "0xd8da6bf26964af9d7eed9e03e53415d37aa96045",
                50_000_000,
                &AssetId::Token("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48".to_string()),
                None,
            )
            .unwrap();
        assert!(!unsigned.is_empty());

        let signed = protocol.sign_transaction(&seed, 0, &unsigned).unwrap();
        assert!(!signed.is_empty());
    }

    #[test]
    fn test_validate_address() {
        let protocol = EthereumProtocol::new(MockRpcClient::new(0, 0));
        assert!(protocol.validate_address("0x9858effd232b4033e47d90003d41ec34ecaeda94"));
        assert!(!protocol.validate_address("invalid"));
    }

    #[test]
    fn test_get_native_balance() {
        let protocol = EthereumProtocol::new(MockRpcClient::new(10_000_000_000_000_000_000, 0));
        let seed = seed_from_mnemonic();
        let pk = protocol.derive_public_key(&seed, 0).unwrap();
        let balance = protocol.get_balance(0, &pk, &AssetId::Native).unwrap();
        assert_eq!(balance.spendable.0, 10_000_000_000_000_000_000);
        assert_eq!(balance.total.0, 10_000_000_000_000_000_000);
        assert_eq!(balance.pending.0, 0);
    }

    #[test]
    fn test_get_erc20_balance() {
        let protocol = EthereumProtocol::new(MockRpcClient::new(0, 5_000_000_000_000_000_000));
        let seed = seed_from_mnemonic();
        let pk = protocol.derive_public_key(&seed, 0).unwrap();
        let balance = protocol
            .get_balance(
                0,
                &pk,
                &AssetId::Token("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48".to_string()),
            )
            .unwrap();
        assert_eq!(balance.spendable.0, 5_000_000_000_000_000_000);
        assert_eq!(balance.total.0, 5_000_000_000_000_000_000);
        assert_eq!(balance.pending.0, 0);
    }
}
