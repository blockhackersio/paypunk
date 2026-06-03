//! Reference example: full shielded Zcash wallet flow against lightwalletd.
//!
//! Demonstrates: seed generation, key derivation, SQLite wallet init, chain sync,
//! balance check, shielded transfer proposal + creation, tx submission, re-sync.
//!
//! Run: cargo run --example send_shielded -- <LIGHTWALLETD_URL> <DEST_ADDRESS>
//! Example: cargo run --example send_shielded -- https://mainnet.lightwalletd.com:9067 u1...

use std::path::PathBuf;
use std::str::FromStr;

use bip39::{Language, Mnemonic};
use rand_core::OsRng;
use rusqlite::Connection;
use zcash_address::unified::{self, Encoding, Receiver};
use zcash_address::{ToAddress, ZcashAddress};
use zcash_client_backend::data_api::wallet::{
    create_proposed_transactions, propose_standard_transfer_to_address, SpendingKeys,
};
use zcash_client_backend::data_api::{AccountBirthday, AccountPurpose, WalletRead, WalletWrite};
use zcash_client_backend::keys::UnifiedSpendingKey;
use zcash_client_backend::proto::service::compact_tx_streamer_client::CompactTxStreamerClient;
use zcash_client_backend::sync;
use zcash_client_sqlite::chain::init::init_cache_database;
use zcash_client_sqlite::util::SysClock;
use zcash_client_sqlite::WalletDb;
use zcash_keys::keys::UnifiedFullViewingKey;
use zcash_protocol::consensus::{BlockHeight, NetworkType, Parameters};
use zcash_protocol::PoolType;
use zcash_protocol::{Memo, MemoBytes, Zatoshis};

/// Helper to create an in-memory SQLite cache DB and return the connection.
fn create_cache_db() -> Connection {
    let conn = Connection::open_in_memory().expect("in-memory cache db");
    conn.execute_batch("PRAGMA journal_mode=wal; PRAGMA synchronous=NORMAL;")
        .ok();
    init_cache_database(&conn).expect("init cache db");
    conn
}

/// Generate a fresh 12-word BIP39 seed, returning (seed_bytes, mnemonic_phrase).
fn generate_seed() -> ([u8; 64], String) {
    let mnemonic = Mnemonic::generate_in(Language::English, 12).expect("generate mnemonic");
    let seed = mnemonic.to_seed_normalized("");
    let phrase = mnemonic.to_string();
    let mut seed_bytes = [0u8; 64];
    seed_bytes.copy_from_slice(&seed[..64]);
    (seed_bytes, phrase)
}

/// Derive a unified address (Orchard-only for simplicity) from a UFVK.
fn derive_unified_address(ufvk: &UnifiedFullViewingKey, network: NetworkType) -> String {
    let (address, _index) = ufvk.default_address().expect("default address");
    let raw = address.to_raw_address_bytes();
    let ua = unified::Address::try_from_items(vec![Receiver::Orchard(raw)])
        .expect("unified address");
    ZcashAddress::from_unified(network, ua).encode()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    // ── CLI args ──────────────────────────────────────────────────────────
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <LIGHTWALLETD_URL> <DEST_ADDRESS>", args[0]);
        std::process::exit(1);
    }
    let lightwalletd_url = &args[1];
    let dest_addr_str = &args[2];

    // ── Network ───────────────────────────────────────────────────────────
    // Use Testnet for testing; switch to NetworkType::Main for mainnet.
    let network = NetworkType::Test;
    let params = zcash_protocol::consensus::TEST_NETWORK;
    let coin_type = 1; // Testnet cointype; use 133 for mainnet

    // ── Step 1: Generate seed & derive keys ───────────────────────────────
    println!("=== Step 1: Generate seed & derive keys ===");
    let (seed, phrase) = generate_seed();
    println!("Mnemonic: {phrase}");

    let usk = UnifiedSpendingKey::from_seed(&seed, coin_type).expect("derive USK");
    let ufvk = UnifiedFullViewingKey::from(&usk);
    let my_address = derive_unified_address(&ufvk, network);
    println!("My address: {my_address}");

    // ── Step 2: Initialize SQLite databases ───────────────────────────────
    println!("\n=== Step 2: Initialize SQLite databases ===");
    let cache_db = create_cache_db();
    let data_dir = PathBuf::from("/tmp/paypunk-reference");
    std::fs::create_dir_all(&data_dir).ok();
    let db_path = data_dir.join("wallet.db");

    let mut wallet_db = WalletDb::for_path(&db_path, params, SysClock, OsRng)
        .expect("create wallet db");
    println!("Wallet DB: {:?}", db_path);

    // ── Step 3: Create account ────────────────────────────────────────────
    println!("\n=== Step 3: Create account ===");
    let birthday = AccountBirthday::from_treestate(
        BlockHeight::from_u32(2800000), // testnet birthday; adjust as needed
        Vec::new(),                     // Sapling frontier (empty for simplicity)
        None,                           // Orchard frontier
    )
    .expect("birthday");
    let account_id = wallet_db
        .create_account(
            &ufvk,
            &AccountPurpose::PreCip18(AccountPurpose::PreCip18 {
                birthday,
                key_source: zcash_client_backend::data_api::AccountSource::Derived(
                    zip32::AccountId::ZERO,
                ),
            }),
        )
        .expect("create account");
    println!("Account ID: {account_id:?}");

    // ── Step 4: Connect to lightwalletd & sync ────────────────────────────
    println!("\n=== Step 4: Connect to lightwalletd & sync ===");
    let mut client = CompactTxStreamerClient::connect(lightwalletd_url.to_string())
        .await
        .expect("connect to lightwalletd");
    println!("Connected to {lightwalletd_url}");

    println!("Syncing wallet...");
    sync::run(&mut client, &params, &cache_db, &mut wallet_db, 100)
        .await
        .expect("sync failed");
    println!("Sync complete!");

    // ── Step 5: Check balance ─────────────────────────────────────────────
    println!("\n=== Step 5: Check balance ===");
    let balance = wallet_db
        .get_account_balance(account_id)
        .expect("get balance");
    println!(
        "Balance: spendable={} zat, pending={} zat, total={} zat",
        balance.spendable().total().into_u64(),
        balance.pending().total().into_u64(),
        balance.total().total().into_u64(),
    );

    if balance.spendable().total() == Zatoshis::ZERO {
        println!("No spendable funds. Send some testnet ZEC to {my_address} and re-run.");
        println!("Exiting without sending a transfer.");
        return Ok(());
    }

    // ── Step 6: Propose & create a shielded transfer ──────────────────────
    println!("\n=== Step 6: Send shielded transfer ===");
    let amount = Zatoshis::from_u64(100_000).expect("100_000 zat"); // 0.001 ZEC
    let dest_addr = ZcashAddress::from_str(dest_addr_str).expect("parse dest address");

    use zcash_client_backend::data_api::wallet::ConfirmationsPolicy;
    use zcash_client_backend::fees::StandardFeeRule;

    let proposal = propose_standard_transfer_to_address(
        &mut wallet_db,
        &params,
        StandardFeeRule::Zip317,
        account_id,
        ConfirmationsPolicy::MinConfirmations,
        &dest_addr,
        amount,
        None, // no memo
        None, // no change memo
        zcash_protocol::ShieldedProtocol::Orchard,
        None, // default tx version
    )
    .expect("propose transfer");
    println!("Proposal created: {} transactions", proposal.steps().len());

    let spending_keys = SpendingKeys { usk };
    let tx_ids = create_proposed_transactions(
        &mut wallet_db,
        &params,
        &zcash_proofs::bundled_prover::BundledProver::new(
            &sapling_crypto::SaplingParameters::new().expect("sapling params"),
            &orchard::builder::build_params(),
        ),
        &proposal,
        &spending_keys,
    )
    .expect("create proposed transactions");
    println!("Transaction created: {:?}", tx_ids);

    // ── Step 7: Submit transaction(s) via lightwalletd ────────────────────
    for tx_id in &tx_ids {
        let tx = wallet_db
            .get_transaction(tx_id)
            .expect("get tx")
            .expect("tx exists");
        let raw_tx = tx.transaction();

        use zcash_client_backend::proto::service::RawTransaction;
        use tonic::Request;

        let submit_req = RawTransaction {
            data: raw_tx.to_vec(),
            height: 0,
        };
        client
            .submit_raw_transaction(Request::new(submit_req))
            .await
            .expect("submit tx");
        println!("Submitted: {tx_id}");
    }

    // ── Step 8: Re-sync & check balance again ─────────────────────────────
    println!("\n=== Step 8: Re-sync & check balance ===");
    sync::run(&mut client, &params, &cache_db, &mut wallet_db, 100)
        .await
        .expect("re-sync failed");

    let balance_after = wallet_db
        .get_account_balance(account_id)
        .expect("get balance after");
    println!(
        "Balance after: spendable={} zat, pending={} zat, total={} zat",
        balance_after.spendable().total().into_u64(),
        balance_after.pending().total().into_u64(),
        balance_after.total().total().into_u64(),
    );

    println!("\nDone! Reference flow complete.");
    Ok(())
}
