pub mod address;
pub mod protocol;

#[cfg(feature = "wallet")]
pub mod wallet_actor;
#[cfg(feature = "wallet")]
pub mod wallet_client;

/// Return the standard Zcash derivation path for a given account index.
///
/// Zcash uses ZIP32 for per-account key derivation. The path identifies the
/// account; addresses are derived from the resulting `UnifiedSpendingKey`
/// using diversifier indices (not BIP44 address-level indices).
///
/// Path: `m/44'/133'/{account}'`
pub fn derivation_path(account: u32) -> String {
    format!("m/44'/133'/{account}'")
}
