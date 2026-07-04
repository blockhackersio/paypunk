#!/usr/bin/env bash
# zcash.sh — start the regtest stack, wait for readiness, and fund an Orchard UA.
# Leaves the node running in the background for wallet testing.
#
# Edit UA below to your wallet's Orchard unified address (uregtest1...).
set -euo pipefail

# should be the first address fron test test test test test test test test test test test junk
UA="uregtest1p8jnyvgzh5dczns4e7ke4v3wgswh32pls057q2tf9mjl2n3372smalr3crg5kjz9x26nyzjyhqq9tm5n9k8pn6ep4fqzu5r2rg052dny"
BLOCKS="${1:-120}"

cleanup() {
  local rc=$?
  if [ $rc -ne 0 ]; then
    echo "!! zcash.sh failed (exit $rc)."
    echo "   Run 'docker compose down' to stop containers."
  fi
}
trap cleanup EXIT

cd ./support/zcash

docker compose down -v
echo "==> Starting zcashd + lightwalletd…"
docker compose up -d --build

echo "==> Waiting for zcashd RPC to be ready…"
for i in $(seq 1 60); do
  if docker compose exec -T zcashd bash -c 'exec 3<>/dev/tcp/127.0.0.1/18232' 2>/dev/null; then
    echo "   zcashd ready after ${i}s"
    break
  fi
  sleep 2
done

echo "==> Funding ${UA}…"
./fund.sh "$UA" "$BLOCKS"

echo ""
echo "==> Stack is up and funded."
echo "    zcashd RPC:      127.0.0.1:18232"
echo "    lightwalletd:    127.0.0.1:9067 (plaintext)"
echo "    Stop with:       docker compose down"
echo "    Wipe + restart:  make reset && make up"
