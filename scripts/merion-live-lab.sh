#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: Apache-2.0
#
# Lab release gate for Keep 0.1 live proof.
# Run on the FluxVM host (or with SSH tunnel to :7788).
#
#   ./scripts/merion-live-lab.sh
#   ./scripts/merion-pilot-gate.sh   # happy + deny, archived logs
#
# Pre-rename names still work: KEEP_<X> is used when MERION_<X> is unset.
[ -n "${MERION_E2E_BIN+x}" ] || [ -z "${KEEP_E2E_BIN:-}" ] || export MERION_E2E_BIN="$KEEP_E2E_BIN"
[ -n "${MERION_E2E_FLUXVM+x}" ] || [ -z "${KEEP_E2E_FLUXVM:-}" ] || export MERION_E2E_FLUXVM="$KEEP_E2E_FLUXVM"
[ -n "${MERION_E2E_TEMPLATE+x}" ] || [ -z "${KEEP_E2E_TEMPLATE:-}" ] || export MERION_E2E_TEMPLATE="$KEEP_E2E_TEMPLATE"
[ -n "${MERION_PILOT_MODE+x}" ] || [ -z "${KEEP_PILOT_MODE:-}" ] || export MERION_PILOT_MODE="$KEEP_PILOT_MODE"
[ -n "${MERION_SIGN_BIN+x}" ] || [ -z "${KEEP_SIGN_BIN:-}" ] || export MERION_SIGN_BIN="$KEEP_SIGN_BIN"

set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

export MERION_E2E_FLUXVM=1
export ZYVOR_AGENT_FLUXVM_URL_LIVE="${ZYVOR_AGENT_FLUXVM_URL_LIVE:-http://127.0.0.1:7788}"
export MERION_E2E_TEMPLATE="${MERION_E2E_TEMPLATE:-}"
export MERION_PILOT_MODE="${MERION_PILOT_MODE:-happy}"

echo "==> Keep 0.1 live lab gate"
echo "    FluxVM: $ZYVOR_AGENT_FLUXVM_URL_LIVE"
echo "    Template override: ${MERION_E2E_TEMPLATE:-auto}"
echo "    Pilot mode: $MERION_PILOT_MODE"

# Prefer release binary when present
if [[ -x agent-runtime/target/release/zyvor-fabric-agent-runtime ]]; then
  export MERION_E2E_BIN="${MERION_E2E_BIN:-$ROOT/agent-runtime/target/release/zyvor-fabric-agent-runtime}"
fi
if [[ -x agent-runtime/target/release/examples/keep_sign_policy ]]; then
  export MERION_SIGN_BIN="${MERION_SIGN_BIN:-$ROOT/agent-runtime/target/release/examples/keep_sign_policy}"
fi

exec ./scripts/merion-e2e.sh
