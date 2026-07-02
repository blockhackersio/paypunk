#!/bin/sh
set -e

RPC_HOST="${ZCASHD_HOST:-zcashd}"
RPC_PORT="${ZCASHD_PORT:-18232}"
INTERVAL="${BLOCK_INTERVAL:-3}"

echo "block-miner: waiting for zcashd at $RPC_HOST:$RPC_PORT..."
until zcash-cli -regtest=1 -rpcconnect="$RPC_HOST" -rpcport="$RPC_PORT" \
  -rpcuser="$RPC_USER" -rpcpassword="$RPC_PASS" getblockchaininfo >/dev/null 2>&1; do
  sleep 1
done
echo "block-miner: zcashd is ready, generating 1 block every ${INTERVAL}s"

while true; do
  zcash-cli -regtest=1 -rpcconnect="$RPC_HOST" -rpcport="$RPC_PORT" \
    -rpcuser="$RPC_USER" -rpcpassword="$RPC_PASS" generate 1 >/dev/null 2>&1
  sleep "$INTERVAL"
done
