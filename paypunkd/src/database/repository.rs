use paypunk_types::Account;
use rusqlite::Connection;

pub trait Repository<T> {
    fn save(&self, conn: &Connection, entity: &T) -> Result<(), String>;
    fn find_all(&self, conn: &Connection) -> Result<Vec<T>, String>;
}

pub trait AccountsRepository: Send + Sync {
    fn save(&self, conn: &Connection, account: &Account) -> Result<(), String>;
    fn find_all(&self, conn: &Connection) -> Result<Vec<Account>, String>;
    fn find_by_id(&self, conn: &Connection, id: &str) -> Result<Option<Account>, String>;
}

pub struct SqliteAccountsRepository;

impl AccountsRepository for SqliteAccountsRepository {
    fn save(&self, conn: &Connection, account: &Account) -> Result<(), String> {
        conn.execute(
            "INSERT INTO accounts (id, protocol, derivation_path, name, viewing_key, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                account.id,
                format!("{:?}", account.protocol),
                account.derivation_path,
                account.name,
                account.viewing_key,
                account.created_at,
            ],
        )
        .map_err(|e| format!("failed to save account: {e}"))?;
        Ok(())
    }

    fn find_all(&self, conn: &Connection) -> Result<Vec<Account>, String> {
        let mut stmt = conn
            .prepare("SELECT id, protocol, derivation_path, name, viewing_key, created_at FROM accounts")
            .map_err(|e| format!("failed to prepare query: {e}"))?;
        let rows = stmt
            .query_map([], |row| {
                let protocol_str: String = row.get(1)?;
                Ok(Account {
                    id: row.get(0)?,
                    protocol: parse_protocol(&protocol_str),
                    derivation_path: row.get(2)?,
                    name: row.get(3)?,
                    viewing_key: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })
            .map_err(|e| format!("failed to query accounts: {e}"))?;
        let mut accounts = Vec::new();
        for row in rows {
            accounts.push(row.map_err(|e| format!("failed to read account row: {e}"))?);
        }
        Ok(accounts)
    }

    fn find_by_id(&self, conn: &Connection, id: &str) -> Result<Option<Account>, String> {
        let mut stmt = conn
            .prepare("SELECT id, protocol, derivation_path, name, viewing_key, created_at FROM accounts WHERE id = ?1")
            .map_err(|e| format!("failed to prepare query: {e}"))?;
        let mut rows = stmt
            .query_map(rusqlite::params![id], |row| {
                let protocol_str: String = row.get(1)?;
                Ok(Account {
                    id: row.get(0)?,
                    protocol: parse_protocol(&protocol_str),
                    derivation_path: row.get(2)?,
                    name: row.get(3)?,
                    viewing_key: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })
            .map_err(|e| format!("failed to query account: {e}"))?;
        match rows.next() {
            Some(Ok(account)) => Ok(Some(account)),
            Some(Err(e)) => Err(format!("failed to read account row: {e}")),
            None => Ok(None),
        }
    }
}

fn parse_protocol(s: &str) -> paypunk_types::ProtocolId {
    match s {
        "Zcash" => paypunk_types::ProtocolId::Zcash,
        "Bitcoin" => paypunk_types::ProtocolId::Bitcoin,
        "Ethereum" => paypunk_types::ProtocolId::Ethereum,
        "Monero" => paypunk_types::ProtocolId::Monero,
        "Solana" => paypunk_types::ProtocolId::Solana,
        _ => paypunk_types::ProtocolId::Zcash,
    }
}
