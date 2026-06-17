pub mod client;
pub mod functions;

pub use client::Client;
pub use functions::{
    approve_signature, derive_address, generate_seed, get_balance, get_balance_legacy,
    restore_seed, submit_intent,
};
