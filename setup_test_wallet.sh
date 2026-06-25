#!/usr/bin/env bash
set -euo pipefail

PAYPUNK="${PAYPUNK_BIN:-cargo run --quiet --package paypunk --}"

MNEMONIC="test test test test test test test test test test test junk"
PASSWORD="test"

echo "Resetting wallet data..."
$PAYPUNK reset

echo "Restoring wallet with test mnemonic..."
$PAYPUNK restore-seed --mnemonic "$MNEMONIC" --password "$PASSWORD"

echo "Unlocking wallet and deriving accounts..."
$PAYPUNK unlock --password "$PASSWORD"

echo "Done. Test wallet ready — password: $PASSWORD"