# Goal 3: Accounts

## Context

The wallet needs multi-account support. An Account represents a derived key within a protocol's HD wallet hierarchy. Each account has:
- A `ProtocolId` (Zcash, Ethereum, etc.)
- An HD derivation path (e.g., `m/44'/133'/0'` for Zcash account 0)
- A human-readable name/label
- A cached viewing key (derived from the seed via keypunkd, stored so we don't need to re-derive on every login)

The existing `export_viewing_key` flow in `keypunkd` already supports deriving viewing keys:
- `keypunkd/src/usecases.rs:100-111`: `export_viewing_key()` calls `SignerProtocol::export_viewing(seed, &path)` 
- `keypunkd/src/messages.rs:32-37`: `ExportViewingKey` request takes `encrypted_password`, `client_public_key`, `protocol`, `account`
- `paypunkd/src/usecases.rs:33-43`: `export_viewing_key()` forwards to keypunkd via IPC
- `paypunkd/src/services.rs:104-124`: `export_viewing_key()` sends `PaypunkdRequest::DeriveAddress`

The existing `types/src/lib.rs` has core types like `ProtocolId`, `Address`, etc. The `Account` type should be added here.

## Implementation plan

### 1. Add `Account` type to `paypunk-types`

In `types/src/lib.rs`, add:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub protocol: ProtocolId,
    pub derivation_path: String,
    pub name: String,
    pub viewing_key: Vec<u8>,
    pub created_at: u64,
}
```

### 2. Create `AccountsRepository` in `paypunkd`

In `paypunkd/src/database/repository.rs`, add the account-specific repository:

```rust
pub trait AccountsRepository: Send + Sync {
    fn save(&self, conn: &Connection, account: &Account) -> Result<(), String>;
    fn find_all(&self, conn: &Connection) -> Result<Vec<Account>, String>;
    fn find_by_id(&self, conn: &Connection, id: &str) -> Result<Option<Account>, String>;
}

pub struct SqliteAccountsRepository;

impl AccountsRepository for SqliteAccountsRepository {
    // Implement using rusqlite
}
```

Schema migration for accounts table (add to migration system):

```sql
CREATE TABLE IF NOT EXISTS accounts (
    id TEXT PRIMARY KEY,
    protocol TEXT NOT NULL,
    derivation_path TEXT NOT NULL,
    name TEXT NOT NULL,
    viewing_key BLOB NOT NULL,
    created_at INTEGER NOT NULL
);
```

### 3. Add account usecases to `paypunkd/src/usecases.rs`

```rust
/// Create a new account: derive viewing key from keypunkd, persist to DB.
pub async fn create_account(
    keypunk_service: &KeypunkService,
    db: &Database,
    repo: &impl AccountsRepository,
    password: &str,
    protocol: ProtocolId,
    derivation_path: String,
    account_index: u32,
    name: String,
) -> Result<Account, String> { ... }

/// List all accounts from the database.
pub fn list_accounts(
    db: &Database,
    repo: &impl AccountsRepository,
) -> Result<Vec<Account>, String> { ... }

/// Get a single account by ID.
pub fn get_account(
    db: &Database,
    repo: &impl AccountsRepository,
    id: &str,
) -> Result<Option<Account>, String> { ... }
```

The `create_account` flow:
1. Connect to keypunkd, call `export_viewing_key` with the password, protocol, and account index
2. Receive the viewing key bytes
3. Construct `Account { id: uuid, protocol, derivation_path, name, viewing_key, created_at: now }`
4. Save via `AccountsRepository::save()`
5. Return the account

### 4. Add request/response variants for accounts to `paypunkd/src/messages.rs`

```rust
pub enum PaypunkdRequest {
    // ... existing variants ...
    CreateAccount {
        protocol: ProtocolId,
        derivation_path: String,
        account_index: u32,
        name: String,
    },
    ListAccounts,
    GetAccount { id: String },
}

pub enum PaypunkdResponse {
    // ... existing variants ...
    AccountCreated { account: Account },
    AccountsList { accounts: Vec<Account> },
    AccountFound { account: Option<Account> },
}
```

Note: The password for keypunkd operations will need to come from the API layer. Consider whether the password should be passed in the request or managed via a session/unlock pattern. For MVP, pass it in the request.

### 5. Add handlers in `paypunkd/src/paypunkd.rs`

Add handler methods that call the usecases and return responses.

### 6. Expose via `api/src/functions.rs` and `api/src/client.rs`

Add high-level functions:
- `client.create_account(protocol, derivation_path, account_index, name)`
- `client.list_accounts()`
- `client.get_account(id)`

## Files to create/modify

- `types/src/lib.rs` — add `Account` struct
- `paypunkd/src/database/repository.rs` — add `AccountsRepository` trait + `SqliteAccountsRepository`
- `paypunkd/src/usecases.rs` — add `create_account`, `list_accounts`, `get_account`
- `paypunkd/src/messages.rs` — add account request/response variants
- `paypunkd/src/paypunkd.rs` — add handler methods for account operations
- `paypunkd/src/lib.rs` — export new modules if needed
- `api/src/client.rs` — add account methods
- `api/src/functions.rs` — add account functions

## Tests

### Integration test: `create_account_via_paypunkd`

```rust
#[tokio::test]
async fn test_create_account() {
    let recipient = TestBuilder::new().build();
    let client = Client::with_recipient(recipient);

    // First generate a seed
    let password = Zeroizing::new("hunter2".to_string());
    client.generate_seed(password.clone()).await.unwrap();

    // Create an account
    let account = client.create_account(
        ProtocolId::Zcash,
        "m/44'/133'/0'".to_string(),
        0,
        "My Zcash Wallet".to_string(),
    ).await.unwrap();

    assert_eq!(account.protocol, ProtocolId::Zcash);
    assert_eq!(account.name, "My Zcash Wallet");
    assert!(!account.viewing_key.is_empty());
    assert!(!account.id.is_empty());
}
```

### Integration test: `list_accounts_returns_created_accounts`

```rust
#[tokio::test]
async fn test_list_accounts() {
    let recipient = TestBuilder::new().build();
    let client = Client::with_recipient(recipient);

    let password = Zeroizing::new("hunter2".to_string());
    client.generate_seed(password.clone()).await.unwrap();

    // Create two accounts
    let acct1 = client.create_account(ProtocolId::Zcash, "m/44'/133'/0'".into(), 0, "Zcash 1".into()).await.unwrap();
    let acct2 = client.create_account(ProtocolId::Ethereum, "m/44'/60'/0'".into(), 0, "Ethereum 1".into()).await.unwrap();

    let accounts = client.list_accounts().await.unwrap();
    assert_eq!(accounts.len(), 2);
    assert!(accounts.iter().any(|a| a.id == acct1.id));
    assert!(accounts.iter().any(|a| a.id == acct2.id));
}
```

## Acceptance criteria

- `Account` type exists in `paypunk-types` with the specified fields
- `AccountsRepository` trait + `SqliteAccountsRepository` impl
- `create_account` usecase sends `ExportViewingKey` to keypunkd, stores result
- `list_accounts` usecase returns all accounts from DB
- Integration test: create an account via paypunkd (using mock keypunkd IPC), verify it's stored and retrievable
