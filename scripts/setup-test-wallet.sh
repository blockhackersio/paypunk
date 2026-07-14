#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
MNEMONIC_FILE="${SCRIPT_DIR}/.mnemonic"
MNEMONIC_DEFAULT="${SCRIPT_DIR}/.mnemonic.example"

PAYPUNK="${PAYPUNK_BIN:-cargo run --quiet --package paypunk --}"

if [ -f "$MNEMONIC_FILE" ]; then
  MNEMONIC=$(cat "$MNEMONIC_FILE")
  NETWORK_ARGS="--zcash-network mainnet"
else
  MNEMONIC=$(cat "$MNEMONIC_DEFAULT")
  NETWORK_ARGS=""
fi

PASSWORD="test"

echo "Resetting wallet data..."
$PAYPUNK reset $NETWORK_ARGS

echo "Restoring wallet with test mnemonic..."
$PAYPUNK restore-seed --mnemonic "$MNEMONIC" --password "$PASSWORD" $NETWORK_ARGS

echo "Unlocking wallet and deriving accounts..."
$PAYPUNK unlock --password "$PASSWORD" $NETWORK_ARGS

echo "Done. Test wallet ready — password: $PASSWORD"