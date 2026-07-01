#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/support/zcash"
exec docker compose up "$@"
