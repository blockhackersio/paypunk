use std::env;

use orchard::keys::{FullViewingKey, Scope, SpendingKey};
use zcash_address::unified::{self, Encoding};
use zcash_address::{ToAddress, ZcashAddress};
use zcash_protocol::consensus::NetworkType;

const DEFAULT_MNEMONIC: &str =
    "test test test test test test test test test test test junk";

fn derive_orchard_address(seed: &[u8; 64], account: u32, index: u32, net: NetworkType) -> String {
    let account_id = zip32::AccountId::try_from(account).expect("valid account");
    let sk = SpendingKey::from_zip32_seed(seed, 133, account_id).expect("ZIP-32 derivation");
    let fvk = FullViewingKey::from(&sk);
    let address = fvk.address_at(index, Scope::External);
    let raw = address.to_raw_address_bytes();
    let ua = unified::Address::try_from_items(vec![unified::Receiver::Orchard(raw)])
        .expect("valid UA");
    let zaddr = ZcashAddress::from_unified(net, ua);
    zaddr.encode()
}

fn main() {
    let mnemonic = env::var("MNEMONIC").unwrap_or_else(|_| DEFAULT_MNEMONIC.to_string());
    let mnemonic =
        bip39::Mnemonic::parse_in(bip39::Language::English, &mnemonic).expect("valid mnemonic");
    let seed = mnemonic.to_seed_normalized("");

    let account: u32 = env::var("ACCOUNT")
        .unwrap_or_else(|_| "0".into())
        .parse()
        .expect("valid account number");
    let index: u32 = env::var("INDEX")
        .unwrap_or_else(|_| "0".into())
        .parse()
        .expect("valid diversifier index");

    // Regtest reuses testnet address prefixes, so use NetworkType::Test.
    let net = NetworkType::Test;
    let ua = derive_orchard_address(&seed, account, index, net);
    println!("{ua}");
}
