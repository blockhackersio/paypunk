#!/usr/bin/env bash
set -euo pipefail

ZCASHD_HOST="${ZCASHD_HOST:-zcashd}"
ZCASHD_PORT="${ZCASHD_PORT:-18232}"
RPC_USER="${RPC_USER:-zcashrpc}"
RPC_PASS="${RPC_PASS:-notsecure}"
BLOCKS="${BLOCKS_TO_MINE:-200}"
NUM_KEYS="${NUM_KEYS:-5}"
SHIELD="${SHIELD_FUNDS:-false}"

log()  { echo -e "\033[1;34m[setup]\033[0m $*"; }
warn() { echo -e "\033[1;33m[warn]\033[0m $*"; }

# zcash-cli requires a config file to exist even when all params are on CLI
mkdir -p /root/.zcash
cat > /root/.zcash/zcash.conf <<-EOF
regtest=1
rpcuser=${RPC_USER}
rpcpassword=${RPC_PASS}
EOF

zcli() {
  zcash-cli \
    -regtest \
    -rpcconnect="$ZCASHD_HOST" \
    -rpcport="$ZCASHD_PORT" \
    -rpcuser="$RPC_USER" \
    -rpcpassword="$RPC_PASS" \
    "$@"
}

# ── Wait for zcashd ──────────────────────────────────────────────────

log "Waiting for zcashd at ${ZCASHD_HOST}:${ZCASHD_PORT}…"

attempts=0
until zcli getblockchaininfo &>/dev/null; do
  attempts=$((attempts + 1))
  if (( attempts > 120 )); then
    echo "[setup] ERROR: zcashd not ready after 120s" >&2
    exit 1
  fi
  sleep 1
done
log "zcashd is ready."

# ── Derive keys ──────────────────────────────────────────────────────

log "Deriving ${NUM_KEYS} transparent addresses from test mnemonic…"
KEYS_JSON=$(node /app/derive-keys.mjs --all --json)

MINING_ADDR=$(echo "$KEYS_JSON" | node -e "
  const k = JSON.parse(require('fs').readFileSync('/dev/stdin','utf8'));
  console.log(k[0].taddr);
")
log "Primary mining address: ${MINING_ADDR}"

# ── Import private keys ─────────────────────────────────────────────

log "Importing private keys…"
for i in $(seq 0 $((NUM_KEYS - 1))); do
  WIF=$(echo "$KEYS_JSON" | node -e "
    const k = JSON.parse(require('fs').readFileSync('/dev/stdin','utf8'));
    console.log(k[${i}].wif);
  ")
  ADDR=$(echo "$KEYS_JSON" | node -e "
    const k = JSON.parse(require('fs').readFileSync('/dev/stdin','utf8'));
    console.log(k[${i}].taddr);
  ")

  if zcli importprivkey "$WIF" "hardhat-${i}" false 2>/dev/null; then
    log "  [$i] ${ADDR} ✓"
  else
    warn "  [$i] ${ADDR} – already imported or error"
  fi
done

# ── Mine blocks ──────────────────────────────────────────────────────

CURRENT=$(zcli getblockcount)
if (( CURRENT < BLOCKS )); then
  NEEDED=$((BLOCKS - CURRENT))
  log "Mining ${NEEDED} blocks…"
  zcli generate "$NEEDED" >/dev/null
else
  log "Already at block ${CURRENT}, skipping."
fi

log "Rescanning wallet…"
zcli rescanblockchain 0 >/dev/null 2>&1 || zcli rescanblockchain >/dev/null 2>&1 || true

BALANCE=$(zcli getbalance)

# ── Shield funds (optional) ─────────────────────────────────────────

if [[ "$SHIELD" == "true" ]]; then
  log "Shielding 50 ZEC into Orchard…"

  UA_RESULT=$(zcli z_getaddressforaccount 0 '["orchard"]' 2>/dev/null || true)
  if [[ -n "$UA_RESULT" ]]; then
    UA=$(echo "$UA_RESULT" | node -e "
      const r = JSON.parse(require('fs').readFileSync('/dev/stdin','utf8'));
      console.log(r.address);
    ")
    log "Unified address: ${UA}"

    OPID=$(zcli z_sendmany "$MINING_ADDR" \
      "[{\"address\":\"${UA}\",\"amount\":50}]" \
      1 null "AllowRevealedSenders" 2>/dev/null || true)

    if [[ -n "$OPID" ]]; then
      while true; do
        STATUS=$(zcli z_getoperationstatus "[\"${OPID}\"]" | node -e "
          const r = JSON.parse(require('fs').readFileSync('/dev/stdin','utf8'));
          console.log(r[0]?.status ?? 'unknown');
        ")
        [[ "$STATUS" == "success" ]] && break
        [[ "$STATUS" == "failed" ]] && { warn "Shielding failed"; break; }
        sleep 1
      done
      zcli generate 1 >/dev/null
    fi
  fi
fi

# ── Summary ──────────────────────────────────────────────────────────

HEIGHT=$(zcli getblockcount)
BALANCE=$(zcli getbalance)

log ""
log "══════════════════════════════════════════════════════════"
log "  Regtest environment ready!"
log ""
log "  Mnemonic     : test test test test test test test test"
log "                 test test test junk"
log "  Derivation   : m/44'/133'/0'/0/{0..$(( NUM_KEYS - 1 ))}"
log "  Mining addr  : ${MINING_ADDR}"
log "  Balance      : ${BALANCE} ZEC"
log "  Block height : ${HEIGHT}"
log ""
log "  zcashd RPC   : ${ZCASHD_HOST}:${ZCASHD_PORT}"
log "  lightwalletd : localhost:9067 (gRPC, no TLS)"
log "══════════════════════════════════════════════════════════"
log ""
log "  All derived addresses (WIF private keys):"
echo "$KEYS_JSON" | node -e "
  const k = JSON.parse(require('fs').readFileSync('/dev/stdin','utf8'));
  k.forEach((a,i) => console.log('    [' + i + '] ' + a.taddr + '  WIF: ' + a.wif));
"
log ""

# Write a marker file so healthcheck or other containers can detect
# that setup is complete
echo "$MINING_ADDR" > /tmp/setup-complete
log "Setup complete."
