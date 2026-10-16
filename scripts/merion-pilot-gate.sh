#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: Apache-2.0
#
# Keep 0.1 pilot release gate — run on the FluxVM host twice:
#   happy path (approve) + deny path. Archives logs under docs/merion/pilot-runs/.
#
#   ./scripts/merion-pilot-gate.sh
#
# Pre-rename names still work: KEEP_<X> is used when MERION_<X> is unset.
[ -n "${MERION_E2E_BIN+x}" ] || [ -z "${KEEP_E2E_BIN:-}" ] || export MERION_E2E_BIN="$KEEP_E2E_BIN"
[ -n "${MERION_E2E_FLUXVM+x}" ] || [ -z "${KEEP_E2E_FLUXVM:-}" ] || export MERION_E2E_FLUXVM="$KEEP_E2E_FLUXVM"
[ -n "${MERION_E2E_TEMPLATE+x}" ] || [ -z "${KEEP_E2E_TEMPLATE:-}" ] || export MERION_E2E_TEMPLATE="$KEEP_E2E_TEMPLATE"
[ -n "${MERION_PILOT_KEEP_LOGS+x}" ] || [ -z "${KEEP_PILOT_KEEP_LOGS:-}" ] || export MERION_PILOT_KEEP_LOGS="$KEEP_PILOT_KEEP_LOGS"
[ -n "${MERION_PILOT_MODE+x}" ] || [ -z "${KEEP_PILOT_MODE:-}" ] || export MERION_PILOT_MODE="$KEEP_PILOT_MODE"
[ -n "${MERION_SIGN_BIN+x}" ] || [ -z "${KEEP_SIGN_BIN:-}" ] || export MERION_SIGN_BIN="$KEEP_SIGN_BIN"

set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

STAMP=$(date -u +%Y%m%dT%H%M%SZ)
BASE="$ROOT/docs/merion/pilot-runs/$STAMP"
mkdir -p "$BASE/happy" "$BASE/deny"

export MERION_E2E_FLUXVM=1
export ZYVOR_AGENT_FLUXVM_URL_LIVE="${ZYVOR_AGENT_FLUXVM_URL_LIVE:-http://127.0.0.1:7788}"
# Prefer Firecracker cell when registered; override with MERION_E2E_TEMPLATE.
if [[ -z "${MERION_E2E_TEMPLATE:-}" ]]; then
  if [[ -d /var/lib/fluxvm/templates/node22-fc ]]; then
    export MERION_E2E_TEMPLATE=node22-fc
  else
    export MERION_E2E_TEMPLATE=node22-agent
  fi
fi

if [[ -x agent-runtime/target/release/zyvor-fabric-agent-runtime ]]; then
  export MERION_E2E_BIN="${MERION_E2E_BIN:-$ROOT/agent-runtime/target/release/zyvor-fabric-agent-runtime}"
fi
if [[ -x agent-runtime/target/release/examples/keep_sign_policy ]]; then
  export MERION_SIGN_BIN="${MERION_SIGN_BIN:-$ROOT/agent-runtime/target/release/examples/keep_sign_policy}"
fi

echo "==> Keep 0.1 pilot gate @ $STAMP"
echo "    template=$MERION_E2E_TEMPLATE flux=$ZYVOR_AGENT_FLUXVM_URL_LIVE"

echo
echo "======== HAPPY PATH ========"
MERION_PILOT_MODE=happy MERION_PILOT_KEEP_LOGS="$BASE/happy" ./scripts/merion-e2e.sh
echo happy_ok >"$BASE/happy/RESULT"

echo
echo "======== DENY PATH ========"
MERION_PILOT_MODE=deny MERION_PILOT_KEEP_LOGS="$BASE/deny" ./scripts/merion-e2e.sh
echo deny_ok >"$BASE/deny/RESULT"

cat >"$BASE/SUMMARY.md" <<EOF
# Keep 0.1 pilot run $STAMP

- Host FluxVM: \`$ZYVOR_AGENT_FLUXVM_URL_LIVE\`
- Template: \`$MERION_E2E_TEMPLATE\`
- Happy path: PASS (see \`happy/\`)
- Deny path: PASS (see \`deny/\`)
- Evidence class: **software-test** (not a TEE claim)

EOF

echo
echo "OK — Keep 0.1 pilot gate passed twice"
echo "Logs: $BASE"
