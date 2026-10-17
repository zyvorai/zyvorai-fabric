# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: Apache-2.0
# Builds zyvor-fabricd + fabricctl (Rust) and the web console (Node), then
# assembles a single runtime image. See docs/DOCKER.md for how to run it
# with Docker or Podman, and why it needs network_mode: host + the specific
# capability list in docker-compose.yml (this Dockerfile only builds the
# image; it doesn't grant any of that).

FROM node:26-slim AS web-builder

WORKDIR /build/web
COPY web/package.json web/package-lock.json ./
RUN npm ci
COPY docs/merion/marketing.json /build/docs/merion/marketing.json
COPY web/ ./
RUN npm run build

FROM rust:1.98-slim AS rust-builder

WORKDIR /build

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    libpam0g-dev \
    libclang-dev \
    clang \
    && rm -rf /var/lib/apt/lists/*

COPY backend/ ./

RUN cargo build --locked --release --bin zyvor-fabricd --bin fabricctl

FROM debian:bookworm-slim

# ca-certificates: outbound TLS (image downloads, etc).
# libpam0g: zyvor-fabricd links against libpam at runtime (the `pam` crate) --
#   without this the binary fails to start, not just falls back gracefully.
# nftables / iproute2: nftables-based firewall/NAT/policy/QoS enforcement and
#   bridge/VLAN/TAP creation (see backend/networking -- bridge/VLAN/macvtap
#   creation goes through rtnetlink directly, but `ip tuntap add` for TAP
#   devices and `nft` for every policy subsystem are still real subprocess
#   calls). Only takes effect against the real host network when the
#   container runs with network_mode: host -- see docker-compose.yml.
# curl: container healthcheck.
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libpam0g \
    nftables \
    iproute2 \
    curl \
    && rm -rf /var/lib/apt/lists/*

RUN mkdir -p /var/lib/zyvor-fabricd/images /var/lib/zyvor-fabricd/storage /etc/zyvor-fabricd \
    /usr/share/zyvor-fabricd/web

COPY --from=rust-builder /build/target/release/zyvor-fabricd /usr/local/bin/
COPY --from=rust-builder /build/target/release/fabricctl /usr/local/bin/
COPY --from=web-builder /build/web/dist/ /usr/share/zyvor-fabricd/web/
COPY configs/zyvor-fabricd-docker.toml /etc/zyvor-fabricd/zyvor-fabricd.toml

EXPOSE 9095

CMD ["/usr/local/bin/zyvor-fabricd"]
