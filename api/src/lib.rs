pub mod client;
pub mod functions;

pub use client::Client;
pub use functions::{generate_seed, restore_seed, unlock, derive_address, sign, lock, get_balance};
