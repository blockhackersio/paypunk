pub mod client;
pub mod functions;

pub use client::Client;
pub use functions::{
    approve_signature, broadcast_transaction, check_wallet_exists, create_account, derive_address,
    generate_seed, get_account, get_balance, get_balance_legacy, list_accounts, restore_seed,
    submit_intent, unlock,
};
