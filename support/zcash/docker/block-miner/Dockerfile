FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
      ca-certificates \
      curl \
      gnupg && \
    curl -fsSL https://apt.z.cash/ > /etc/apt/trusted.gpg.d/zcash.asc && \
    echo "deb [arch=amd64] https://apt.z.cash/ bookworm main" > /etc/apt/sources.list.d/zcash.list && \
    apt-get update && \
    apt-get install -y --no-install-recommends zcash && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/*

COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

ENTRYPOINT ["/entrypoint.sh"]
