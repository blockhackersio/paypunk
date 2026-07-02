FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates curl gnupg apt-transport-https && \
    curl -fsSL https://apt.z.cash/zcash.asc | gpg --dearmor -o /usr/share/keyrings/zcash-archive-keyring.gpg && \
    echo "deb [signed-by=/usr/share/keyrings/zcash-archive-keyring.gpg] https://apt.z.cash/ bookworm main" \
      > /etc/apt/sources.list.d/zcash.list && \
    apt-get update && \
    apt-get install -y --no-install-recommends zcash && \
    apt-get clean && rm -rf /var/lib/apt/lists/*

COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

ENTRYPOINT ["/entrypoint.sh"]
