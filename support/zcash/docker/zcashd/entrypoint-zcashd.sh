#!/usr/bin/env bash
set -euo pipefail

DATA_DIR="/data"

# If params were not baked into the image, fetch them now
if [[ ! -f "${HOME}/.zcash-params/sapling-spend.params" ]]; then
  echo "[zcashd] Fetching Zcash parameters…"
  zcash-fetch-params
fi

echo "[zcashd] Starting in regtest mode…"
exec zcashd \
  -datadir="$DATA_DIR" \
  -printtoconsole \
  -debug=rpc
