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

# ── Derive keys for display ──────────────────────────────────────────

log "Deriving ${NUM_KEYS} transparent addresses from test mnemonic…"
KEYS_JSON=$(node /app/derive-keys.mjs --all --json)

MINING_ADDR=$(echo "$KEYS_JSON" | node -e "
  const k = JSON.parse(require('fs').readFileSync('/dev/stdin','utf8'));
  console.log(k[0].taddr);
")
log "Primary mining address: ${MINING_ADDR}"

# ── Mine blocks ──────────────────────────────────────────────────────
# Mining to the wallet's own transparent address (zcashd 6.x wallet
# derives its own keys from a random mnemonic; we mine to its address
# so it has spendable coinbase UTXOs).

CURRENT=$(zcli getblockcount)
if (( CURRENT < BLOCKS )); then
  NEEDED=$((BLOCKS - CURRENT))
  log "Mining ${NEEDED} blocks (coinbase goes to wallet's internal address)…"
  zcli generate "$NEEDED" >/dev/null
else
  log "Already at block ${CURRENT}, skipping."
fi

log "Rescanning wallet…"
zcli rescanblockchain 0 >/dev/null 2>&1 || zcli rescanblockchain >/dev/null 2>&1 || true

# Wait for rescan to complete before proceeding
log "Waiting for rescan to complete…"
for i in $(seq 1 120); do
  SCANNING=$(zcli getblockchaininfo 2>/dev/null | node -e "
    const r = JSON.parse(require('fs').readFileSync('/dev/stdin','utf8'));
    console.log(JSON.stringify(r.scanning));
  " 2>/dev/null || echo "unknown")
  if [[ "$SCANNING" == "false" ]] || [[ -z "$SCANNING" ]] || [[ "$SCANNING" == "null" ]]; then
    log "Rescan complete."
    break
  fi
  sleep 2
done

# ── Shield coinbase funds into Orchard ───────────────────────────────
#
# ORCHARD_UA is a uregtest1... address derived from the test mnemonic,
# matching what paypunkd derives. We use z_shieldcoinbase to move
# coinbase UTXOs into the Orchard pool at that address.

if [[ -n "${ORCHARD_UA:-}" ]]; then
  log "Shielding coinbase funds to wallet Orchard UA: ${ORCHARD_UA}…"

  SHIELD_RESULT=$(zcli z_shieldcoinbase "*" "${ORCHARD_UA}" null 0 null "AllowRevealedSenders" 2>/dev/null || true)

  if [[ -n "$SHIELD_RESULT" ]]; then
    OPID=$(echo "$SHIELD_RESULT" | node -e "
      const r = JSON.parse(require('fs').readFileSync('/dev/stdin','utf8'));
      console.log(r.opid);
    ")

    if [[ -n "$OPID" ]]; then
      log "Shielding operation: ${OPID}, waiting…"
      while true; do
        STATUS=$(zcli z_getoperationstatus "[\"${OPID}\"]" | node -e "
          const r = JSON.parse(require('fs').readFileSync('/dev/stdin','utf8'));
          console.log(r[0]?.status ?? 'unknown');
        ")
        [[ "$STATUS" == "success" ]] && break
        [[ "$STATUS" == "failed" ]] && { warn "Shielding failed"; break; }
        sleep 2
      done
      zcli generate 1 >/dev/null
      zcli generate 1 >/dev/null
      log "Shielding complete."
    fi
  else
    warn "z_shieldcoinbase failed"
  fi

elif [[ "$SHIELD" == "true" ]]; then
  log "Shielding 50 ZEC into Orchard (zcashd-internal UA)…"

  UA_RESULT=$(zcli z_getaddressforaccount 0 '["orchard"]' 2>/dev/null || true)
  if [[ -n "$UA_RESULT" ]]; then
    UA=$(echo "$UA_RESULT" | node -e "
      const r = JSON.parse(require('fs').readFileSync('/dev/stdin','utf8'));
      console.log(r.address);
    ")
    log "Unified address: ${UA}"

    SHIELD_RESULT=$(zcli z_shieldcoinbase "*" "${UA}" null 0 null "AllowRevealedSenders" 2>/dev/null || true)
    if [[ -n "$SHIELD_RESULT" ]]; then
      OPID=$(echo "$SHIELD_RESULT" | node -e "
        const r = JSON.parse(require('fs').readFileSync('/dev/stdin','utf8'));
        console.log(r.opid);
      ")
      if [[ -n "$OPID" ]]; then
        while true; do
          STATUS=$(zcli z_getoperationstatus "[\"${OPID}\"]" | node -e "
            const r = JSON.parse(require('fs').readFileSync('/dev/stdin','utf8'));
            console.log(r[0]?.status ?? 'unknown');
          ")
          [[ "$STATUS" == "success" ]] && break
          [[ "$STATUS" == "failed" ]] && { warn "Shielding failed"; break; }
          sleep 2
        done
        zcli generate 1 >/dev/null
        zcli generate 1 >/dev/null
      fi
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
if [[ -n "${ORCHARD_UA:-}" ]]; then
  log ""
  log "  Wallet Orchard UA (funds shielded):"
  log "    ${ORCHARD_UA}"
fi
log ""

# Write a marker file so healthcheck or other containers can detect
# that setup is complete
echo "$MINING_ADDR" > /tmp/setup-complete
log "Setup complete."
