#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: Apache-2.0
#
# One-click Keep demo: drop a file into a sealed cell, get an artifact back
# (no browser, expect 0 CONNECT).
#
#   ./scripts/merion-demo.sh list
#   ./scripts/merion-demo.sh <demo-id> [file]
#
# With no file, the demo's sample under examples/merion-agents/<id>/ is used.
# Pre-rename names still work: KEEP_<X> is used when MERION_<X> is unset.
[ -n "${MERION_API+x}" ] || [ -z "${KEEP_API:-}" ] || export MERION_API="$KEEP_API"
[ -n "${MERION_TOKEN+x}" ] || [ -z "${KEEP_TOKEN:-}" ] || export MERION_TOKEN="$KEEP_TOKEN"

set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
: "${MERION_API:=${ZYVOR_AGENT_URL:-http://127.0.0.1:9096}}"
AUTH=()
if [[ -n "${MERION_TOKEN:-${ZYVOR_AGENT_TOKEN:-}}" ]]; then
  AUTH=(-H "Authorization: Bearer ${MERION_TOKEN:-$ZYVOR_AGENT_TOKEN}")
fi

usage() {
  echo "usage: $0 list | $0 <demo-id> [file]" >&2
  exit 64
}
[[ $# -ge 1 ]] || usage

echo "==> preflight"
curl -fsS "${AUTH[@]}" "$MERION_API/healthz" >/dev/null || {
  echo "runtime /healthz failed" >&2
  exit 1
}

if [[ "$1" == "list" ]]; then
  curl -fsS "${AUTH[@]}" "$MERION_API/v1/demos" | python3 -c '
import json, sys
for d in json.load(sys.stdin)["demos"]:
    print("%-24s .%s  %s" % (d["id"], "/.".join(d["accepts"]), d["description"]))'
  exit 0
fi

ID=$1
[[ "$ID" =~ ^[a-z0-9-]+$ ]] || { echo "bad demo id: $ID" >&2; exit 64; }
FILE=${2:-}
if [[ -z "$FILE" ]]; then
  # shellcheck disable=SC2012
  FILE=$(ls "$ROOT/examples/merion-agents/$ID"/sample.* 2>/dev/null | head -1 || true)
fi
test -n "$FILE" && test -f "$FILE" || { echo "missing input file for $ID: ${FILE:-none}" >&2; exit 1; }

echo "==> POST /v1/demos/$ID  ($(basename "$FILE"))"
OUT=$(curl -fsS "${AUTH[@]}" -X POST -F "file=@${FILE}" "$MERION_API/v1/demos/$ID")
echo "$OUT" | python3 -m json.tool
SID=$(echo "$OUT" | python3 -c "import json,sys; print(json.load(sys.stdin)['session_id'])")
CONN=$(echo "$OUT" | python3 -c "import json,sys; print(json.load(sys.stdin).get('egress_connects', -1))")
ARTS=$(echo "$OUT" | python3 -c "import json,sys; d=json.load(sys.stdin); print(', '.join(a['title'] for a in d.get('artifacts', [])) or d.get('artifact_title', ''))")
echo "COCKPIT $MERION_API (console /app/merion/$SID)"
if [[ "$CONN" != "0" ]]; then
  echo "FAIL: egress_connects=$CONN" >&2
  exit 2
fi
SEALED=$(echo "$OUT" | python3 -c "import json,sys; print(str(json.load(sys.stdin).get('badge', {}).get('sealed', True)).lower())")
if [[ "$SEALED" == "false" ]]; then
  # a local simulator (scripts/merion-demo-local.sh), not a FluxVM cell: never present its count as proof
  echo "OK — $ARTS ready. SIMULATED, not sealed: no VM and no network policy, so the connection count is not evidence"
  exit 0
fi
echo "OK — $ARTS ready, 0 CONNECT"
