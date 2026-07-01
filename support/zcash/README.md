# Zcash Regtest Docker Stack

A fully containerized Zcash development environment funded from the
Hardhat "test junk" mnemonic. One command gives you a running `zcashd`
regtest node, `lightwalletd` gRPC endpoint, and pre-funded deterministic
addresses.

## Quick Start

```bash
./start-zcash.sh
```

Or directly:

```bash
docker compose up --build
```

First run takes a while (downloads ~1.7 GB of Zcash parameters + builds
lightwalletd from source). Subsequent runs use cached layers and volumes.

## What Happens

1. **zcashd** starts in regtest mode with all network upgrades (through
   NU5/Orchard) active at block 1.
2. **setup** (one-shot container) derives 5 transparent addresses from
   the mnemonic `test test test ... test junk` using BIP-44
   `m/44'/133'/0'/0/{0..4}` for reference, then mines 200 blocks to the
   wallet's internal transparent address (zcashd 6.x generates its own
   random wallet seed on first run).
3. **setup then shields coinbase UTXOs** into the Orchard UA derived
   from the same mnemonic (ZIP-32, account 0, diversifier index 0) via
   `z_shieldcoinbase` — this is the same address your wallet derives,
   so `paypunkd` will see the balance after syncing.
4. **lightwalletd** starts on port 9067 (gRPC, no TLS), connected to
   zcashd.

## Exposed Ports

| Port  | Service       | Protocol       |
|-------|---------------|----------------|
| 9067  | lightwalletd  | gRPC (no TLS)  |
| 18232 | zcashd        | JSON-RPC       |

## Usage

```bash
# Start with automatic Orchard shielding (recommended — gives your wallet 100 ZEC)
./start-zcash.sh

# Start detached
./start-zcash.sh -d

# Start with extra ZEC shielded into Orchard (via SHIELD_FUNDS fallback)
SHIELD_FUNDS=true docker compose up --build

# Mine more blocks
docker compose exec zcashd \
  zcash-cli -datadir=/data -rpcuser=zcashrpc -rpcpassword=notsecure \
  generatetoaddress 10 <MINING_ADDR>

# Check balance
docker compose exec zcashd \
  zcash-cli -datadir=/data -rpcuser=zcashrpc -rpcpassword=notsecure \
  getbalance

# Full reset (wipe all data)
docker compose down -v
docker compose up --build

# View setup logs
docker compose logs setup
```

## Connecting Your Client

Point any lightwalletd-compatible client at:

```
host: 127.0.0.1
port: 9067
TLS:  disabled
```

The gRPC API (`GetBlockRange`, `GetTransaction`, `SendTransaction`, etc.)
is identical to mainnet.

## File Structure

```
├── docker-compose.yml
└── docker/
    ├── zcashd/
    │   ├── Dockerfile
    │   ├── zcash.conf
    │   └── entrypoint-zcashd.sh
    ├── lightwalletd/
    │   ├── Dockerfile
    │   ├── zcash-lwd.conf
    │   └── entrypoint-lwd.sh
    └── setup/
        ├── Dockerfile
        ├── setup-init.sh
        ├── derive-keys.mjs
        └── package.json
```
