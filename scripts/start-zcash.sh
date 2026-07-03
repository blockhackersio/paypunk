#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

# ── Derive the Orchard UA from the Hardhat test mnemonic ──────────────
echo "[start-zcash] Building derive-orchard tool…"
cargo build -p derive-orchard --quiet 2>/dev/null || cargo build -p derive-orchard 2>&1

ORCHARD_UA=$(cargo run -p derive-orchard --quiet 2>/dev/null | jq -r '.ua')
echo "[start-zcash] Orchard UA: ${ORCHARD_UA}"

export ORCHARD_UA

# ── Launch the Zcash regtest stack ────────────────────────────────────
cd support/zcash
exec docker compose up --build "$@"
