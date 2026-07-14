#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
MNEMONIC_FILE="${SCRIPT_DIR}/.mnemonic"
MNEMONIC_DEFAULT="${SCRIPT_DIR}/.mnemonic.example"

PAYPUNK="${PAYPUNK_BIN:-cargo run --quiet --package paypunk --}"

if [ -f "$MNEMONIC_FILE" ]; then
  MNEMONIC=$(cat "$MNEMONIC_FILE")
else
  MNEMONIC=$(cat "$MNEMONIC_DEFAULT")
fi

PASSWORD="test"

echo "Resetting wallet data..."
$PAYPUNK reset

echo "Restoring wallet with test mnemonic..."
$PAYPUNK restore-seed --mnemonic "$MNEMONIC" --password "$PASSWORD"

echo "Unlocking wallet and deriving accounts..."
$PAYPUNK unlock --password "$PASSWORD"

echo "Done. Test wallet ready — password: $PASSWORD"