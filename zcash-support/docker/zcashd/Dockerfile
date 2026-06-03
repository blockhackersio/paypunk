# ── zcashd regtest node ───────────────────────────────────────────────
# Installs zcashd from the official ECC apt repository, fetches the
# Sprout/Sapling parameters, and starts in regtest mode.
#
# The params download (~1.7 GB) is cached in a named volume so it only
# happens on the first run.

FROM debian:bookworm-slim AS zcashd

ARG ZCASH_VERSION=5.10.0

RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates curl gnupg apt-transport-https && \
    # Add the Zcash apt repo
    curl -fsSL https://apt.z.cash/zcash.asc | gpg --dearmor -o /usr/share/keyrings/zcash-archive-keyring.gpg && \
    echo "deb [signed-by=/usr/share/keyrings/zcash-archive-keyring.gpg] https://apt.z.cash/ bookworm main" \
      > /etc/apt/sources.list.d/zcash.list && \
    apt-get update && \
    apt-get install -y --no-install-recommends zcash && \
    apt-get clean && rm -rf /var/lib/apt/lists/*

# Fetch Sprout/Sapling params at build time so the layer is cached.
# If you'd rather defer to runtime (and cache via volume), comment this
# out and uncomment the fetch in entrypoint.sh instead.
RUN zcash-fetch-params

RUN mkdir -p /data/regtest

COPY zcash.conf /data/zcash.conf
COPY entrypoint-zcashd.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

EXPOSE 18232 18233

ENTRYPOINT ["/entrypoint.sh"]
