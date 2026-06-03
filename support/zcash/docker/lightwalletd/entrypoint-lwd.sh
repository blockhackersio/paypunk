#!/usr/bin/env bash
set -euo pipefail

ZCASHD_HOST="${ZCASHD_HOST:-zcashd}"
ZCASHD_PORT="${ZCASHD_PORT:-18232}"
CONF_PATH="${CONF_PATH:-/etc/lightwalletd/zcash.conf}"
GRPC_BIND="${GRPC_BIND:-0.0.0.0:9067}"

echo "[lwd] Waiting for zcashd RPC at ${ZCASHD_HOST}:${ZCASHD_PORT}…"
attempts=0
until nc -z "$ZCASHD_HOST" "$ZCASHD_PORT" 2>/dev/null; do
  attempts=$((attempts + 1))
  if (( attempts > 120 )); then
    echo "[lwd] ERROR: zcashd RPC not available after 120s" >&2
    exit 1
  fi
  sleep 1
done
echo "[lwd] zcashd RPC is reachable."

# Give zcashd a few more seconds to fully initialize after the port opens
sleep 3

echo "[lwd] Starting lightwalletd on ${GRPC_BIND}…"
exec lightwalletd \
  --zcash-conf-path="$CONF_PATH" \
  --data-dir=/data \
  --log-file=/dev/stdout \
  --grpc-bind-addr="$GRPC_BIND" \
  --no-tls-very-insecure
