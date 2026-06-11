This document outlines the end state for the project architecture.

### Intent layer create at the API

We start with an intent layer that is created at the API level

> The intent layer speaks in on-chain addresses—for both accounts and assets. Human-friendly abstractions (derivation paths, token symbols) get resolved to addresses; they aren't the primary key.

The main expected intent will be a transfer initially - later we will include swaps which will be directed in the paypunkd layer - but not initially

1. API assembles a chain specific intent instruction - see CAIP-2 CAIP-19
2. paypunkd - receives the serialized intent over ipc parses it and send it to the appropriate protocol
3. protocol/* - implements the `Protocol` trait uses `build(intent)` to build a signable artifact
    - parses the intent and fails if it does not parse
    - executes the intent 
    - utilizes any backend service it has been configured with that it requires eg.
        - chain db that holds a cache of received synced notes
        - rpc client that can be queried to get balances
4. paypunkd - receives artifact and forwards bytes to keypunkd
5. keypunkd - receives artifact and authorizes the artifact this includes a UX flow that parses and shows what is being signed (help on this design)
6. keypunkd - completes the authorization step with the entry of a password returns the signed artifact to paypunkd
7. paypunkd - calls finalize on the signed bytes and makes it broadcast ready then uses the chain backend to broadcast the transaction.

To accomplish this we should refactor `Protocol` and `SignerProtocol` to the following traits:

```rust
trait Protocol: Send + Sync {
    fn protocol_id(&self) -> ProtocolId; // CAIP-2 
    async fn build(&self, intent: &[u8]) -> Result<Vec<u8>, String>; // -> canonical UNSIGNED artifact
    async fn finalize(&self, signed: &[u8]) -> Result<Vec<u8>, String>; // -> broadcast-ready bytes
    fn validate_address(&self, addr: &str) -> bool;
}
```

```rust
trait SignerProtocol: Send + Sync {
    fn chain(&self) -> ChainId;
    fn export_viewing(&self, path: &[u8]) -> Result<Vec<u8>, String>;   // xpub / ed25519 pub / Orchard FVK
    async fn authorize(&self, artifact: &[u8]) -> Result<Vec<u8>, String>;    // RE-PARSES the artifact
}
```

Inside each impl, the first thing you do is deserialize `&[u8]` into your strong concrete type, work in real types, and serialize back at the exit. **The trait is generic; the implementations are specific.** Bytes only at the seams.

Note bytes have been used to avoid associated types perhaps this is unnecessary as we could use an Intent enum that covers all possible intents. Ideally the intent structure will be chain agnostic assuming we make use of CAIP-2 / 19

The main intents that matter initially will be:

Intent {
    Transfer {
        asset: String,
        amount: String,
        from: String,
        to: String,
        memo: String
    }
    // Later 
    ContractCall
}
