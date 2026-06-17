```rust
// ============================================================================
// wallet_usecases.rs
//
// Flat catalogue of function signatures representing every core use case
// of a non-custodial wallet. Chain-agnostic: covers account-model (Solana),
// UTXO-model (BTC), and privacy chains (Zcash shielded, Monero).
//
// No trait grouping imposed — consume these however your architecture needs.
// ============================================================================

// ── Primitives ──────────────────────────────────────────────────────────────
//
// These are referenced throughout; every chain pins concrete types to them.
//
//   Address          — chain-native address (transparent, shielded, stealth)
//   PublicKey         — the public half of a keypair
//   SpendKey         — key that authorises spending
//   ViewKey          — key that decrypts incoming txs without spend authority
//   Signature
//   UnsignedTx
//   SignedTx
//   TxHash
//   TxReceipt
//   TokenId          — SPL mint / Omni asset / Zcash asset id
//   Amount
//   DerivationPath
//   Mnemonic
//   Seed
//   MasterKey
//   NetworkId
//   Error

// ════════════════════════════════════════════════════════════════════════════
//  MNEMONIC & SEED
// ════════════════════════════════════════════════════════════════════════════

fn generate_mnemonic(word_count: u8) -> Result<Mnemonic, Error>;
fn mnemonic_from_phrase(phrase: &str) -> Result<Mnemonic, Error>;
fn validate_mnemonic(phrase: &str) -> bool;
fn mnemonic_to_seed(mnemonic: &Mnemonic, passphrase: &str) -> Seed;

// ════════════════════════════════════════════════════════════════════════════
//  KEY DERIVATION
// ════════════════════════════════════════════════════════════════════════════

fn master_key_from_seed(seed: &Seed) -> Result<MasterKey, Error>;
fn derive_spend_key(master: &MasterKey, path: &DerivationPath) -> Result<SpendKey, Error>;
fn derive_view_key(spend_key: &SpendKey) -> Result<ViewKey, Error>;
// ↑ Monero/Zcash: view key derived from spend key
// ↑ BTC/Solana: returns a trivial/identity view key (everything is public)
fn public_key_from_spend_key(spend_key: &SpendKey) -> PublicKey;
fn address_from_public_key(public_key: &PublicKey) -> Address;
fn default_derivation_path() -> DerivationPath;

// ════════════════════════════════════════════════════════════════════════════
//  ADDRESS GENERATION
// ════════════════════════════════════════════════════════════════════════════

fn generate_transparent_address(public_key: &PublicKey) -> Address;
fn generate_shielded_address(spend_key: &SpendKey) -> Result<Address, Error>;
// ↑ Zcash sapling/orchard, Monero stealth address
// ↑ BTC/Solana: not applicable, returns Error or aliases to transparent
fn generate_one_time_address(spend_key: &SpendKey, recipient_pub: &PublicKey) -> Result<Address, Error>;
// ↑ Monero-style: sender generates per-tx stealth address for recipient
fn is_own_address(view_key: &ViewKey, address: &Address) -> bool;
// ↑ scan whether an address/output belongs to this wallet

// ════════════════════════════════════════════════════════════════════════════
//  ACCOUNT / KEY STORE
// ════════════════════════════════════════════════════════════════════════════

fn derive_next_account(store: &mut KeyStore) -> Result<AccountHandle, Error>;
fn derive_account_at(store: &mut KeyStore, index: u32) -> Result<AccountHandle, Error>;
fn import_spend_key(store: &mut KeyStore, key: SpendKey) -> Result<AccountHandle, Error>;
fn import_view_only(store: &mut KeyStore, view_key: ViewKey, address: Address) -> Result<AccountHandle, Error>;
// ↑ watch-only: can scan incoming txs, cannot spend
fn remove_account(store: &mut KeyStore, address: &Address) -> Result<SpendKey, Error>;
fn list_accounts(store: &KeyStore) -> Vec<AccountHandle>;
fn export_spend_key(store: &KeyStore, address: &Address) -> Result<SpendKey, Error>;
fn export_view_key(store: &KeyStore, address: &Address) -> Result<ViewKey, Error>;

// ════════════════════════════════════════════════════════════════════════════
//  NETWORK CONFIGURATION
// ════════════════════════════════════════════════════════════════════════════

fn get_active_network() -> NetworkId;
fn set_active_network(network: NetworkId) -> Result<(), Error>;
fn get_rpc_endpoint() -> String;
fn set_rpc_endpoint(url: String) -> Result<(), Error>;
fn is_testnet() -> bool;

// ════════════════════════════════════════════════════════════════════════════
//  SIGNING (pure crypto, sync, no network)
// ════════════════════════════════════════════════════════════════════════════

fn sign_raw_bytes(spend_key: &SpendKey, data: &[u8]) -> Result<Signature, Error>;
fn sign_message(spend_key: &SpendKey, message: &[u8]) -> Result<Signature, Error>;
// ↑ human-readable off-chain message signing (proof of ownership, auth)
fn sign_transaction(spend_key: &SpendKey, tx: &UnsignedTx) -> Result<SignedTx, Error>;
fn verify_signature(public_key: &PublicKey, message: &[u8], sig: &Signature) -> bool;

// ════════════════════════════════════════════════════════════════════════════
//  TRANSACTION BUILDING
// ════════════════════════════════════════════════════════════════════════════

fn build_transfer(
    from: &Address,
    to: &Address,
    amount: Amount,
) -> impl Future<Output = Result<UnsignedTx, Error>> + Send;

fn build_transfer_with_memo(
    from: &Address,
    to: &Address,
    amount: Amount,
    memo: &[u8],
) -> impl Future<Output = Result<UnsignedTx, Error>> + Send;
// ↑ Zcash encrypted memo, Monero tx extra, Solana memo program, BTC OP_RETURN

fn build_multi_output_transfer(
    from: &Address,
    recipients: &[(Address, Amount)],
) -> impl Future<Output = Result<UnsignedTx, Error>> + Send;
// ↑ native on UTXO chains; batched on account-model chains

fn build_token_transfer(
    token: &TokenId,
    from: &Address,
    to: &Address,
    amount: Amount,
) -> impl Future<Output = Result<UnsignedTx, Error>> + Send;
// ↑ SPL transfer (Solana), Omni/RGB (BTC), Zcash assets — not all chains support

fn build_token_approve(
    token: &TokenId,
    owner: &Address,
    spender: &Address,
    amount: Amount,
) -> impl Future<Output = Result<UnsignedTx, Error>> + Send;

fn build_token_revoke(
    token: &TokenId,
    owner: &Address,
    spender: &Address,
) -> impl Future<Output = Result<UnsignedTx, Error>> + Send;

// ════════════════════════════════════════════════════════════════════════════
//  UTXO MANAGEMENT (BTC, Zcash transparent, partially Monero)
// ════════════════════════════════════════════════════════════════════════════

fn list_unspent_outputs(
    address: &Address,
) -> impl Future<Output = Result<Vec<Utxo>, Error>> + Send;

fn select_inputs(
    available: &[Utxo],
    target: Amount,
    strategy: CoinSelection,
) -> Result<Vec<Utxo>, Error>;
// ↑ coin selection: minimize fee, maximize privacy, consolidate, etc.

fn estimate_fee(
    tx: &UnsignedTx,
) -> impl Future<Output = Result<Amount, Error>> + Send;

fn set_fee(tx: &mut UnsignedTx, fee: Amount) -> Result<(), Error>;

// ════════════════════════════════════════════════════════════════════════════
//  BROADCASTING
// ════════════════════════════════════════════════════════════════════════════

fn broadcast(tx: SignedTx) -> impl Future<Output = Result<TxHash, Error>> + Send;

fn send(
    from: &Address,
    to: &Address,
    amount: Amount,
    spend_key: &SpendKey,
) -> impl Future<Output = Result<TxHash, Error>> + Send;
// ↑ convenience: build → sign → broadcast

// ════════════════════════════════════════════════════════════════════════════
//  CHAIN READS — Balances & Transaction Status
// ════════════════════════════════════════════════════════════════════════════

fn get_balance(address: &Address) -> impl Future<Output = Result<Amount, Error>> + Send;

fn get_shielded_balance(
    view_key: &ViewKey,
) -> impl Future<Output = Result<Amount, Error>> + Send;
// ↑ Zcash/Monero: must scan chain with view key to tally shielded balance
// ↑ BTC/Solana: aliases to get_balance

fn get_transaction(hash: &TxHash) -> impl Future<Output = Result<Option<TxReceipt>, Error>> + Send;
fn get_transaction_status(hash: &TxHash) -> impl Future<Output = Result<TxStatus, Error>> + Send;
fn get_current_block_height() -> impl Future<Output = Result<u64, Error>> + Send;

// ════════════════════════════════════════════════════════════════════════════
//  CHAIN READS — Tokens
// ════════════════════════════════════════════════════════════════════════════

fn get_token_balance(
    token: &TokenId,
    owner: &Address,
) -> impl Future<Output = Result<Amount, Error>> + Send;

fn get_token_allowance(
    token: &TokenId,
    owner: &Address,
    spender: &Address,
) -> impl Future<Output = Result<Amount, Error>> + Send;

fn get_token_metadata(token: &TokenId) -> impl Future<Output = Result<TokenMetadata, Error>> + Send;

fn discover_tokens(
    owner: &Address,
) -> impl Future<Output = Result<Vec<TokenBalance>, Error>> + Send;
// ↑ Solana: scan token accounts. BTC: Omni/RGB lookup. Zcash: ZSA scan.

// ════════════════════════════════════════════════════════════════════════════
//  CHAIN READS — History
// ════════════════════════════════════════════════════════════════════════════

fn get_transaction_history(
    address: &Address,
    cursor: Option<String>,
    limit: u32,
) -> impl Future<Output = Result<Page<HistoryEntry>, Error>> + Send;

// ════════════════════════════════════════════════════════════════════════════
//  PRIVACY — Scanning, Proofs, Decoys
// ════════════════════════════════════════════════════════════════════════════

fn scan_for_incoming(
    view_key: &ViewKey,
    from_height: u64,
    to_height: u64,
) -> impl Future<Output = Result<Vec<DetectedOutput>, Error>> + Send;
// ↑ Monero/Zcash: scan blocks to detect outputs owned by this wallet
// ↑ BTC/Solana: no-op or simple address-match filter

fn generate_payment_proof(
    tx_hash: &TxHash,
    spend_key: &SpendKey,
    recipient: &Address,
) -> Result<PaymentProof, Error>;
// ↑ Monero: prove you sent a tx. Zcash: prove shielded payment. BTC/Solana: tx is public.

fn verify_payment_proof(proof: &PaymentProof) -> Result<bool, Error>;

fn select_decoys(
    real_output: &Utxo,
    ring_size: u32,
) -> impl Future<Output = Result<Vec<Utxo>, Error>> + Send;
// ↑ Monero: select ring members. Others: not applicable.

fn generate_key_image(spend_key: &SpendKey, output: &Utxo) -> Result<KeyImage, Error>;
// ↑ Monero: prevent double-spend in ring sig scheme. Others: not applicable.

// ════════════════════════════════════════════════════════════════════════════
//  DAPP CONNECTION
// ════════════════════════════════════════════════════════════════════════════

fn connect_dapp(
    origin: DAppOrigin,
    requested_permissions: &[Permission],
) -> impl Future<Output = Result<SessionId, Error>> + Send;

fn disconnect_dapp(session: &SessionId) -> impl Future<Output = Result<(), Error>> + Send;
fn list_active_sessions() -> Vec<Session>;
fn approve_dapp_request(session: &SessionId, request_id: u64) -> impl Future<Output = Result<(), Error>> + Send;
fn reject_dapp_request(session: &SessionId, request_id: u64) -> impl Future<Output = Result<(), Error>> + Send;

// ════════════════════════════════════════════════════════════════════════════
//  SUPPORTING TYPES (minimal, for signature readability)
// ════════════════════════════════════════════════════════════════════════════

pub enum TxStatus {
    Pending,
    Confirmed { confirmations: u64 },
    Failed { reason: String },
    NotFound,
}

pub enum CoinSelection {
    MinimizeFee,
    MaximizePrivacy,   // avoid merging outputs, prefer fewer inputs
    Consolidate,       // merge dust into fewer outputs
    Manual(Vec<Utxo>), // caller picks exact inputs
}

pub struct Utxo {
    pub tx_hash: TxHash,
    pub output_index: u32,
    pub amount: Amount,
    pub address: Address,
    pub confirmations: u64,
    pub is_shielded: bool,
}

pub struct DetectedOutput {
    pub tx_hash: TxHash,
    pub output_index: u32,
    pub amount: Amount,
    pub block_height: u64,
    pub is_spent: bool,
}

pub struct KeyImage(pub [u8; 32]);
pub struct PaymentProof(pub Vec<u8>);

pub struct TokenMetadata {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
}

pub struct TokenBalance {
    pub token: TokenId,
    pub metadata: TokenMetadata,
    pub balance: Amount,
}

pub struct Page<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

pub struct HistoryEntry {
    pub hash: TxHash,
    pub direction: TxDirection,
    pub counterparty: Address,
    pub amount: Amount,
    pub token: Option<TokenId>,
    pub status: TxStatus,
    pub timestamp: Option<u64>,
}

pub enum TxDirection { Incoming, Outgoing, SelfTransfer }

pub enum Permission { ViewAccounts, SignMessages, SignTransactions, AutoSign }

pub struct AccountHandle {
    pub address: Address,
    pub public_key: PublicKey,
    pub has_spend_key: bool,
    pub has_view_key: bool,
    pub label: Option<String>,
}

pub struct Session {
    pub id: SessionId,
    pub origin: DAppOrigin,
    pub accounts: Vec<Address>,
    pub permissions: Vec<Permission>,
    pub connected_at: u64,
}
```
