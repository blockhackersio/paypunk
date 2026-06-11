pub mod client;
pub mod functions;

pub use client::Client;
pub use functions::{
    generate_seed, restore_seed, unlock, lock, derive_address, submit_intent, approve_signature,
    get_balance, get_balance_legacy,
};
