#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/../.."

PAYPUNK="${PAYPUNK_BIN:-cargo run --quiet --package paypunk --}"
UA="uregtest1p8jnyvgzh5dczns4e7ke4v3wgswh32pls057q2tf9mjl2n3372smalr3crg5kjz9x26nyzjyhqq9tm5n9k8pn6ep4fqzu5r2rg052dny"
KEYPUNKD_SOCK="/tmp/keypunkd.sock"
PAYPUNKD_SOCK="/tmp/paypunkd.sock"

cleanup() {
  local rc=$?
  echo "==> Cleaning up daemons..."
  kill $KEYPUNKD_PID $PAYPUNKD_PID 2>/dev/null || true
  wait $KEYPUNKD_PID $PAYPUNKD_PID 2>/dev/null || true
  rm -f "$KEYPUNKD_SOCK" "$PAYPUNKD_SOCK"
  echo "==> Done (exit $rc)"
}
trap cleanup EXIT

# ── 1. Start Zcash regtest stack ────────────────────────────────────────────
echo "==> Starting Zcash regtest stack..."
./scripts/start-zcash.sh 120

# ── 2. Initialize test wallet ───────────────────────────────────────────────
echo "==> Initializing test wallet..."
./scripts/setup-test-wallet.sh

# ── 3. Start daemons ────────────────────────────────────────────────────────
echo "==> Starting key-daemon..."
$PAYPUNK keypunkd &
KEYPUNKD_PID=$!

echo "==> Starting wallet-daemon..."
$PAYPUNK paypunkd &
PAYPUNKD_PID=$!

# ── 4. Wait for daemon sockets ──────────────────────────────────────────────
echo "==> Waiting for daemon sockets..."
for i in $(seq 1 30); do
  if [ -S "$KEYPUNKD_SOCK" ] && [ -S "$PAYPUNKD_SOCK" ]; then
    echo "   Both daemons ready after ${i}s"
    break
  fi
  sleep 1
done

if [ ! -S "$KEYPUNKD_SOCK" ] || [ ! -S "$PAYPUNKD_SOCK" ]; then
  echo "!! Daemons did not start in time"
  exit 1
fi

# ── 5. Get balance ──────────────────────────────────────────────────────────
echo "==> Querying balance for funded UA..."
BALANCE=$($PAYPUNK get-balance --protocol zcash --address "$UA" 2>&1)
echo "$BALANCE"

# ── 6. Verify balance is non-zero ───────────────────────────────────────────
SPENDABLE=$(echo "$BALANCE" | grep -oP 'spendable=\K\d+')
PENDING=$(echo "$BALANCE" | grep -oP 'pending=\K\d+')
TOTAL=$(echo "$BALANCE" | grep -oP 'total=\K\d+')

if [ -z "$TOTAL" ] || [ "$TOTAL" -eq 0 ]; then
  echo "!! FAIL: balance is zero — expected funded account to have funds"
  exit 1
fi

echo "==> PASS: balance = $TOTAL zatoshi (spendable=$SPENDABLE, pending=$PENDING)"
