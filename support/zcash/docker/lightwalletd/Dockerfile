# ── lightwalletd ──────────────────────────────────────────────────────
# Builds lightwalletd from source and runs it against the zcashd
# regtest node.

FROM golang:1.22-bookworm AS builder

ARG LWD_VERSION=v0.4.17

RUN apt-get update && apt-get install -y --no-install-recommends git && \
    git clone --depth 1 --branch "$LWD_VERSION" \
      https://github.com/zcash/lightwalletd.git /src/lightwalletd

WORKDIR /src/lightwalletd
RUN go build -o /usr/local/bin/lightwalletd .

# ── Runtime ───────────────────────────────────────────────────────────

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates netcat-openbsd && \
    apt-get clean && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/local/bin/lightwalletd /usr/local/bin/lightwalletd
RUN chmod +x /usr/local/bin/lightwalletd
COPY entrypoint-lwd.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

RUN mkdir -p /data /etc/lightwalletd

EXPOSE 9067

ENTRYPOINT ["/entrypoint.sh"]
