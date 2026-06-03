# ── Setup container ───────────────────────────────────────────────────
# One-shot container: derives keys from the Hardhat test mnemonic,
# imports them into zcashd, and mines initial blocks.

FROM node:22-bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates curl gnupg apt-transport-https netcat-openbsd && \
    # Install zcash-cli only (we just need the CLI, not the daemon)
    curl -fsSL https://apt.z.cash/zcash.asc | gpg --dearmor -o /usr/share/keyrings/zcash-archive-keyring.gpg && \
    echo "deb [signed-by=/usr/share/keyrings/zcash-archive-keyring.gpg] https://apt.z.cash/ bookworm main" \
      > /etc/apt/sources.list.d/zcash.list && \
    apt-get update && \
    apt-get install -y --no-install-recommends zcash && \
    apt-get clean && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY package.json derive-keys.mjs ./
RUN npm install --production

COPY setup-init.sh /setup-init.sh
RUN chmod +x /setup-init.sh

ENTRYPOINT ["/setup-init.sh"]
