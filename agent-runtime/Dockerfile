# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: Apache-2.0
# Builds the Keep runtime (zyvor-fabric-agent-runtime). Build from the repo root:
#   docker build -f agent-runtime/Dockerfile -t zyvor-fabric-agent-runtime:local agent-runtime
# It only serves HTTP and talks to FluxVM, so it needs no capabilities; see charts/zyvor-merion.
FROM rust:1.98-slim AS build
WORKDIR /build
COPY . .
RUN cargo build --locked --release --bin zyvor-fabric-agent-runtime

# Same Debian release as the rust:1.98-slim build stage (trixie). A bookworm runtime has an older glibc and the
# binary refuses to start (GLIBC_2.38 not found).
FROM debian:trixie-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir -p /var/lib/zyvor-fabric-agent /var/lib/fluxvm/agent-snapshots
COPY --from=build /build/target/release/zyvor-fabric-agent-runtime /usr/local/bin/
ENV ZYVOR_AGENT_STATE_DIR=/var/lib/zyvor-fabric-agent
EXPOSE 9096 18082 18083
HEALTHCHECK --interval=30s --timeout=5s CMD curl -fsS "http://${ZYVOR_AGENT_LISTEN:-127.0.0.1:9096}/healthz" || exit 1
ENTRYPOINT ["/usr/local/bin/zyvor-fabric-agent-runtime"]
