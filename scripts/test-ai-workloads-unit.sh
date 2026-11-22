#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

# Unit tests for Fabric AI Workloads (Phases 2–4 pure logic).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# Remote deploys often keep cargo under ~/.cargo; CI has it on PATH.
if [[ -f "$HOME/.cargo/env" ]]; then
  # shellcheck source=/dev/null
  source "$HOME/.cargo/env"
fi
cd "$ROOT/backend"
# Single filter matches all ai::* unit modules under the fabricd lib.
cargo test -p zyvor-fabricd --lib api::ai:: -- --nocapture
# Focused Preview slices used by the Janus lab path (one filter each).
cargo test -p zyvor-fabricd --lib api::ai::janus:: -- --nocapture
cargo test -p zyvor-fabricd --lib api::ai::gpu_orch:: -- --nocapture
cargo test -p zyvor-fabricd --lib api::ai::gateway::tests::gateway_serves_openai_paths -- --nocapture
