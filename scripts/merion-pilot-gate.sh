#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: Apache-2.0
#
# Keep 0.1 pilot release gate — run on the FluxVM host twice:
#   happy path (approve) + deny path. Archives logs under docs/merion/pilot-runs/.
#
#   ./scripts/merion-pilot-gate.sh
#
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

STAMP=$(date -u +%Y%m%dT%H%M%SZ)
BASE="$ROOT/docs/merion/pilot-runs/$STAMP"
mkdir -p "$BASE/happy" "$BASE/deny"

export KEEP_E2E_FLUXVM=1
export ZYVOR_AGENT_FLUXVM_URL_LIVE="${ZYVOR_AGENT_FLUXVM_URL_LIVE:-http://127.0.0.1:7788}"
# Prefer Firecracker cell when registered; override with KEEP_E2E_TEMPLATE.
if [[ -z "${KEEP_E2E_TEMPLATE:-}" ]]; then
  if [[ -d /var/lib/fluxvm/templates/node22-fc ]]; then
    export KEEP_E2E_TEMPLATE=node22-fc
  else
    export KEEP_E2E_TEMPLATE=node22-agent
  fi
fi

if [[ -x agent-runtime/target/release/zyvor-fabric-agent-runtime ]]; then
  export KEEP_E2E_BIN="${KEEP_E2E_BIN:-$ROOT/agent-runtime/target/release/zyvor-fabric-agent-runtime}"
fi
if [[ -x agent-runtime/target/release/examples/keep_sign_policy ]]; then
  export KEEP_SIGN_BIN="${KEEP_SIGN_BIN:-$ROOT/agent-runtime/target/release/examples/keep_sign_policy}"
fi

echo "==> Keep 0.1 pilot gate @ $STAMP"
echo "    template=$KEEP_E2E_TEMPLATE flux=$ZYVOR_AGENT_FLUXVM_URL_LIVE"

echo
echo "======== HAPPY PATH ========"
KEEP_PILOT_MODE=happy KEEP_PILOT_KEEP_LOGS="$BASE/happy" ./scripts/merion-e2e.sh
echo happy_ok >"$BASE/happy/RESULT"

echo
echo "======== DENY PATH ========"
KEEP_PILOT_MODE=deny KEEP_PILOT_KEEP_LOGS="$BASE/deny" ./scripts/merion-e2e.sh
echo deny_ok >"$BASE/deny/RESULT"

cat >"$BASE/SUMMARY.md" <<EOF
# Keep 0.1 pilot run $STAMP

- Host FluxVM: \`$ZYVOR_AGENT_FLUXVM_URL_LIVE\`
- Template: \`$KEEP_E2E_TEMPLATE\`
- Happy path: PASS (see \`happy/\`)
- Deny path: PASS (see \`deny/\`)
- Evidence class: **software-test** (not a TEE claim)

EOF

echo
echo "OK — Keep 0.1 pilot gate passed twice"
echo "Logs: $BASE"
