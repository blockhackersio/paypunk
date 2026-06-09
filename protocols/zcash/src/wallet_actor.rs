use std::sync::Mutex;

use rand_core::OsRng;
use tactix::{Actor, Ctx, Handler, Message};
use zcash_client_sqlite::util::SystemClock;
use zcash_client_sqlite::WalletDb;

/// Messages sent to the Zcash WalletDbActor.
#[derive(Debug, Message)]
#[response(Result<Vec<u8>, String>)]
pub enum WalletMessage {
    ProposeAndBuild {
        public_key: Vec<u8>,
        account: u32,
        to: String,
        amount: u64,
        memo: Option<String>,
    },
}

/// Tactix actor wrapping `zcash_client_sqlite::WalletDb` behind a Mutex
/// (rusqlite::Connection is !Sync, so we need the Mutex for the Actor bound).
pub struct WalletDbActor {
    pub db: Mutex<
        WalletDb<rusqlite::Connection, zcash_protocol::consensus::Network, SystemClock, OsRng>,
    >,
}

impl WalletDbActor {
    pub fn new(
        db: WalletDb<rusqlite::Connection, zcash_protocol::consensus::Network, SystemClock, OsRng>,
    ) -> Self {
        Self { db: Mutex::new(db) }
    }
}

impl Actor for WalletDbActor {}

impl Handler<WalletMessage> for WalletDbActor {
    async fn handle(
        &mut self,
        msg: WalletMessage,
        _ctx: &Ctx<Self>,
    ) -> Result<Vec<u8>, String> {
        match msg {
            WalletMessage::ProposeAndBuild {
                public_key: _,
                account,
                to: _,
                amount: _,
                memo: _,
            } => Err(format!(
                "propose_and_build requires a fully synced WalletDb with notes. \
                 account={account} is not yet implemented via zcash_client_backend APIs. \
                 This will use propose_standard_transfer_to_address + create_pczt_from_proposal."
            )),
        }
    }
}
