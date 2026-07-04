#!/bin/sh
set -e

RPC_HOST="${ZCASHD_HOST:-zcashd}"
RPC_PORT="${ZCASHD_PORT:-18232}"
INTERVAL="${BLOCK_INTERVAL:-3}"

# zcash-cli requires a config file even when all params are on CLI
mkdir -p /root/.zcash
cat > /root/.zcash/zcash.conf <<-EOF
regtest=1
rpcuser=${RPC_USER}
rpcpassword=${RPC_PASS}
EOF

echo "block-miner: waiting for zcashd at $RPC_HOST:$RPC_PORT..."
until zcash-cli -regtest=1 -rpcconnect="$RPC_HOST" -rpcport="$RPC_PORT" \
  -rpcuser="$RPC_USER" -rpcpassword="$RPC_PASS" getblockchaininfo >/dev/null 2>&1; do
  sleep 1
done
echo "block-miner: zcashd is ready, generating 1 block every ${INTERVAL}s"

while true; do
  if ! zcash-cli -regtest=1 -rpcconnect="$RPC_HOST" -rpcport="$RPC_PORT" \
    -rpcuser="$RPC_USER" -rpcpassword="$RPC_PASS" generate 1 >/dev/null; then
    echo "block-miner: generate failed, retrying..."
    sleep 1
    continue
  fi
  echo "mining block..."
  sleep "$INTERVAL"
done
