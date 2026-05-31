# ADR-001: IPC Authentication Model

**Status**: Accepted
**Date**: 2026-05-31

## Context

keypunkd exposes a Unix socket for IPC. We need to ensure that only the
legitimate paypunkd process can send messages to keypunkd. Several approaches
were considered:

1. **OS-level authentication**: SO_PEERCRED UID/PID checks, parent-PID
   bootstrapping, filesystem permissions on the socket.
2. **Password-only auth**: The wallet password gates sensitive operations after
   `Unlock`, but any process can connect and attempt the handshake.
3. **Cryptographic message signing**: Each process holds a long-lived X25519
   keypair in memory. Every IPC message is authenticated using the shared
   secret derived from the two processes' keys.

We already have X25519 key exchange infrastructure in keypunkd (`KeyStore`,
`CryptoSession`, `derive_aes_key`) used for sealing the password during
`GenerateSeed`. This same infrastructure can be extended for per-message
authentication.

## Decision

**Every IPC message is authenticated by the sender's in-memory process key.**
Each daemon generates a long-lived X25519 keypair at startup. On connection,
the client registers its public key, and both sides derive a shared secret via
X25519 Diffie-Hellman. Every subsequent message includes an authentication tag
derived from this shared secret.

In practice, this means only paypunkd (the holder of its private key) can
produce valid messages — no other process can connect to keypunkd and be
accepted.

### Protocol

1. **keypunkd** generates an X25519 keypair at startup (`KeyStore`, already
   implemented). Exposes its public key via `GetPublicKey`.

2. **paypunkd** generates its own X25519 keypair at startup (new — analogous to
   `KeyStore` but for the app daemon).

3. **Connection handshake**:
   - paypunkd connects to keypunkd's Unix socket.
   - paypunkd calls `GetPublicKey` to learn keypunkd's public key.
   - paypunkd sends `RegisterClient { public_key: [u8; 32] }` with its own
     public key.
   - keypunkd stores paypunkd's public key for this connection and derives a
     shared secret: `X25519(keypunkd_sk, paypunkd_pk)`.
   - paypunkd derives the same shared secret: `X25519(paypunkd_sk, keypunkd_pk)`.

4. **Per-message authentication**:
   - Every subsequent message from paypunkd includes an HMAC tag:
     `message_payload || HMAC(message_payload, hmac_key)` where `hmac_key` is
     derived from the shared secret via Blake2b.
   - keypunkd verifies the HMAC before processing the message. If verification
     fails, the connection is dropped.

5. **Existing password sealing remains unchanged**: The `GenerateSeed` flow
   still uses ephemeral `CryptoSession` keys + X25519 + AES-GCM to encrypt the
   password and mnemonic. The per-message HMAC is an additional layer on every
   message, not a replacement for the encryption of sensitive payloads.

### What this means for each process

| Process | Runs as | Key material |
|---------|---------|-------------|
| keypunkd | Separate system user (or same user in dev) | Long-lived X25519 keypair in `KeyStore`; shared secret per connected client |
| paypunkd | User's login UID | Long-lived X25519 keypair in memory; shared secret per connection to keypunkd |
| paypunk (CLI) | User's login UID | No keys — connects to paypunkd (not keypunkd); paypunkd does not require message-level auth for CLI connections |

### OS user separation

keypunkd runs as a separate system user (e.g., `keypunkd`) for defense-in-depth:
- `seed.enc` is owned by the keypunkd user, not the wallet user
- keypunkd's process memory is isolated by the OS
- A compromise of the wallet user does not directly expose key material

The cryptographic message authentication is independent of user separation —
it works the same whether keypunkd runs as a different user or the same user as
paypunkd.

## Consequences

### Positive

- **Process-level identity**: Only the process holding paypunkd's private key
  can produce valid messages. This is stronger than UID checks (which only
  identify a user, not a specific process) and stronger than password-only auth
  (which requires the password to be sent before any gate).
- **Reuses existing crypto**: X25519 key exchange and Blake2b key derivation
  are already implemented in `crypto.rs`.
- **No OS-specific code**: No SO_PEERCRED, no getpeereid, no platform cfg gates.
- **No secret distribution**: Each process generates its own keypair at startup.
  The public keys are exchanged over the connection — no files, no env vars.
- **Keypairs are ephemeral**: A new keypair is generated on each daemon restart.
  No persistence needed.
- **Referential transparency preserved**: The ipc crate remains a pure transport
  layer. Authentication is handled by the dispatcher at the application level.

### Negative

- **Handshake latency**: The `GetPublicKey` + `RegisterClient` exchange adds a
  round trip at connection startup.
- **Per-connection registration**: Each new connection must re-register. If
  paypunkd reconnects, it generates a new keypair and re-registers.
- **HMAC overhead**: Small CPU cost per message for HMAC computation and
  verification. Negligible for a wallet application.
- **No key persistence**: If an attacker gains temporary access to paypunkd's
  memory, they can extract the current keypair. Mitigated by: (a) the keypair
  is regenerated on restart, (b) the attacker would also need access to
  keypunkd's socket, (c) the wallet password is still required for `Unlock`.

## Implementation Plan

### keypunkd changes

- Add `RegisterClient { public_key: [u8; 32] }` variant to `KeypunkdRequest`
- Add `KeypunkdResponse::ClientRegistered` variant
- Add connection-level state to `Dispatcher`:
  ```rust
  struct ConnectionState {
      client_public_key: [u8; 32],
      hmac_key: [u8; 32],  // derived from shared secret
  }
  ```
- Before processing any message, verify the HMAC tag (except for
  `GetPublicKey` and `RegisterClient` which are part of the handshake)
- Store connection state keyed by something unique per connection (e.g., an
  incrementing connection ID passed through the actor context)

### paypunkd changes (new crate)

- Generate an X25519 keypair at startup
- On connect to keypunkd: `GetPublicKey` → `RegisterClient`
- Derive shared secret and HMAC key
- Before sending any message: compute and append HMAC tag

## Alternatives Considered

### SO_PEERCRED UID checking
keypunkd checks the connecting process's UID against paypunkd's UID. Rejected
because it's platform-specific (Linux-only), only identifies the user not the
process, and adds no meaningful security if both daemons run as the same user
during development.

### Parent-PID bootstrap
paypunkd spawns keypunkd; keypunkd records its parent PID and only accepts
connections from that PID. Rejected because it breaks with sudo (sudo becomes
the parent, not paypunkd) and doesn't work if processes are launched by a
service manager.

### Password-only auth
The wallet password gates sensitive operations after `Unlock`. Rejected as the
sole mechanism because it still allows any process to connect and participate
in the handshake protocol. Message-level auth closes this window entirely.

### Shared-secret handshake
A pre-shared key distributed via file or env var, presented on connection.
Rejected because it adds secret distribution complexity. Ephemeral key exchange
avoids this entirely.
