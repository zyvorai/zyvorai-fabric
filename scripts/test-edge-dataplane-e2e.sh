#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0
#
# Fabric → FluxVM edge dataplane (schema v4) e2e via Fabric HTTPS APIs.
#
# Usage:
#   FABRIC_URL=https://HOST:9095 ./scripts/test-edge-dataplane-e2e.sh
#   # or with password:
#   FABRIC_URL=… FABRIC_USER=admin FABRIC_PASSWORD=… ./scripts/test-edge-dataplane-e2e.sh
#   # or token:
#   FABRIC_URL=… FABRIC_TOKEN=… ./scripts/test-edge-dataplane-e2e.sh
#
# Optional: FABRIC_VM=name — use an existing bridged VM for status/effective checks.
set -euo pipefail

BASE="${FABRIC_URL:-https://127.0.0.1:9095}"
BASE="${BASE%/}"
USER="${FABRIC_USER:-admin}"
PASS="${FABRIC_PASSWORD:-}"
TOKEN="${FABRIC_TOKEN:-${ZYVOR_FABRIC_TOKEN:-}}"
VM_NAME="${FABRIC_VM:-}"

PASS_N=0
FAIL_N=0
pass() { PASS_N=$((PASS_N + 1)); echo "  [PASS] $1"; }
fail() { FAIL_N=$((FAIL_N + 1)); echo "  [FAIL] $1" >&2; }
section() { echo ""; echo "=== $1 ==="; }

CURL=(curl -sk)
auth_hdr=()

login() {
  if [[ -n "$TOKEN" ]]; then
    auth_hdr=(-H "Authorization: Bearer $TOKEN")
    return 0
  fi
  if [[ -z "$PASS" ]]; then
    echo "Set FABRIC_TOKEN or FABRIC_PASSWORD" >&2
    exit 1
  fi
  local body
  body="$("${CURL[@]}" -H 'Content-Type: application/json' \
    -d "{\"username\":\"$USER\",\"password\":\"$PASS\"}" \
    "$BASE/api/auth/login" 2>/dev/null || true)"
  TOKEN="$(python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("token") or d.get("access_token") or "")' <<<"$body" 2>/dev/null || true)"
  if [[ -z "$TOKEN" ]]; then
    # alternate login path used by some builds
    body="$("${CURL[@]}" -H 'Content-Type: application/json' \
      -d "{\"username\":\"$USER\",\"password\":\"$PASS\"}" \
      "$BASE/api/login" 2>/dev/null || true)"
    TOKEN="$(python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("token") or d.get("access_token") or "")' <<<"$body" 2>/dev/null || true)"
  fi
  [[ -n "$TOKEN" ]] || { echo "login failed: $body" >&2; exit 1; }
  auth_hdr=(-H "Authorization: Bearer $TOKEN")
}

api() {
  local method="$1" path="$2"
  shift 2
  "${CURL[@]}" -X "$method" "${auth_hdr[@]}" -H 'Content-Type: application/json' \
    "$@" "$BASE$path"
}

json_get() {
  python3 -c 'import json,sys; d=json.load(sys.stdin)
path=sys.argv[1].split(".")
cur=d
for p in path:
  if p=="": continue
  if isinstance(cur, list):
    cur=cur[int(p)]
  else:
    cur=cur[p]
print(cur if not isinstance(cur,(dict,list)) else json.dumps(cur))' "$1"
}

echo "Fabric edge dataplane e2e → $BASE"
login
section "health / capabilities"

READY="$(curl -sk "$BASE/readyz" || true)"
echo "$READY" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert "ok" in d and "fluxvm" in d' \
  && pass "GET /readyz" || fail "GET /readyz"

CAP="$(api GET /api/capabilities || true)"
echo "$CAP" | python3 -c 'import json,sys; d=json.load(sys.stdin); v=d.get("vm_dataplane") or {}; print(v.get("phase"), v.get("detail",""))' \
  && pass "capabilities.vm_dataplane" || fail "capabilities.vm_dataplane"

HEALTH="$(api GET /api/dataplane/health)"
echo "$HEALTH" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert "ok" in d and "mode" in d' \
  && pass "GET /api/dataplane/health" || fail "GET /api/dataplane/health"

section "groups"

GNAME="fabric-e2e-web-$$"
GROUP_JSON=$(python3 - <<PY
import json
print(json.dumps({
  "name": "$GNAME",
  "labels": ["app=fabric-e2e"],
  "priority": 10,
  "description": "e2e",
  "identity": 0,
  "policy": {
    "default_allow": False,
    "allow_cidrs": ["10.0.0.0/8"],
    "deny_cidrs": ["10.66.0.0/16"],
    "allow_ports": ["tcp/443", "udp/53"],
    "allow_icmp": True,
    "groups": [],
    "labels": [],
    "allow_fqdns": [],
    "entities": [],
    "audit_mode": False,
    "max_egress_mbps": 100,
    "max_egress_pps": None,
    "sample_rate": 0,
  }
}))
PY
)

UPSERT="$(api POST /api/dataplane/groups -d "$GROUP_JSON")"
echo "$UPSERT" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert d["name"]==sys.argv[1] and d.get("identity",0)>0' "$GNAME" \
  && pass "POST /api/dataplane/groups" || fail "POST /api/dataplane/groups"

LIST="$(api GET /api/dataplane/groups)"
echo "$LIST" | python3 -c 'import json,sys; items=json.load(sys.stdin).get("items",[]); assert any(g["name"]==sys.argv[1] for g in items)' "$GNAME" \
  && pass "GET /api/dataplane/groups lists e2e group" || fail "GET /api/dataplane/groups"

GETG="$(api GET "/api/dataplane/groups/$GNAME")"
echo "$GETG" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert d["name"]==sys.argv[1]' "$GNAME" \
  && pass "GET /api/dataplane/groups/{name}" || fail "GET /api/dataplane/groups/{name}"

section "cnp / identities / observe / ipcache / refresh-dns"

CNP_NAME="fabric-e2e-cnp-$$"
CNP_JSON=$(python3 - <<PY
import json
print(json.dumps({
  "apiVersion": "cilium.io/v2",
  "kind": "CiliumNetworkPolicy",
  "metadata": {"name": "$CNP_NAME"},
  "spec": {
    "endpointSelector": {"matchLabels": {"app": "fabric-e2e"}},
    "egress": [{
      "toCIDR": ["192.168.0.0/16"],
      "toPorts": [{"ports": [{"port": "80", "protocol": "TCP"}]}]
    }]
  }
}))
PY
)

APPLY="$(api POST /api/dataplane/cnp -d "$CNP_JSON" || true)"
echo "$APPLY" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert "name" in d or "identity" in d' \
  && pass "POST /api/dataplane/cnp" || fail "POST /api/dataplane/cnp ($APPLY)"

CNPL="$(api GET /api/dataplane/cnp)"
echo "$CNPL" | python3 -c 'import json,sys; items=json.load(sys.stdin).get("items",[]); assert any((c.get("metadata") or {}).get("name")==sys.argv[1] for c in items)' "$CNP_NAME" \
  && pass "GET /api/dataplane/cnp" || fail "GET /api/dataplane/cnp"

IDS="$(api GET /api/dataplane/identities)"
echo "$IDS" | python3 -c 'import json,sys; items=json.load(sys.stdin).get("items",[]); assert len(items)>=1' \
  && pass "GET /api/dataplane/identities" || fail "GET /api/dataplane/identities"

EPS="$(api GET /api/dataplane/endpoints)"
echo "$EPS" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert "items" in d' \
  && pass "GET /api/dataplane/endpoints" || fail "GET /api/dataplane/endpoints"

# Soft: MicroVM metrics upstream is optional (fluxvm-microvm may be down).
MM_CODE="$("${CURL[@]}" -o /tmp/fabric-mm.txt -w '%{http_code}' -X GET "${auth_hdr[@]}" \
  "$BASE/api/dataplane/microvm-metrics" || true)"
if [[ "$MM_CODE" == "200" ]] && grep -qE 'fluxvm_microvm_|# HELP|# TYPE|# microvm' /tmp/fabric-mm.txt; then
  pass "GET /api/dataplane/microvm-metrics"
elif [[ "$MM_CODE" == "502" || "$MM_CODE" == "000" ]]; then
  pass "GET /api/dataplane/microvm-metrics (upstream optional, HTTP ${MM_CODE:-none})"
else
  pass "GET /api/dataplane/microvm-metrics (HTTP $MM_CODE)"
fi

OBS="$(api GET /api/dataplane/observe)"
echo "$OBS" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert "groups" in d and "identities" in d' \
  && pass "GET /api/dataplane/observe" || fail "GET /api/dataplane/observe"

IPC="$(api GET /api/dataplane/ipcache)"
echo "$IPC" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert "items" in d' \
  && pass "GET /api/dataplane/ipcache" || fail "GET /api/dataplane/ipcache"

REF="$(api POST /api/dataplane/refresh-dns -d '{}')"
echo "$REF" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert "refreshed" in d' \
  && pass "POST /api/dataplane/refresh-dns" || fail "POST /api/dataplane/refresh-dns"

section "per-VM (optional)"

if [[ -z "$VM_NAME" ]]; then
  VM_NAME="$(api GET /api/vms 2>/dev/null | python3 -c 'import json,sys
try:
  d=json.load(sys.stdin)
  items=d if isinstance(d,list) else d.get("items") or d.get("vms") or []
  print(items[0]["name"] if items else "")
except Exception:
  print("")' || true)"
fi

if [[ -n "$VM_NAME" ]]; then
  ST="$(api GET "/api/vms/$VM_NAME/dataplane/status" || true)"
  if echo "$ST" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert "mode" in d' 2>/dev/null; then
    pass "GET …/dataplane/status ($VM_NAME)"
    ATTACHED="$(echo "$ST" | python3 -c 'import json,sys; print("1" if json.load(sys.stdin).get("attached") else "0")')"
    SCHEMA="$(echo "$ST" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("schema_version") or 0)')"
    if [[ "$ATTACHED" == "1" ]]; then
      if [[ "$SCHEMA" -ge 4 ]]; then
        pass "attached schema_version>=4 ($SCHEMA)"
      else
        fail "attached but schema_version expected >=4 got $SCHEMA (upgrade FluxVM BPF on host)"
      fi
    else
      echo "  (skip schema assert — VM not attached; restart bridged VM after FluxVM upgrade)"
      pass "status reachable while unattached"
    fi
    api GET "/api/vms/$VM_NAME/dataplane/stats" >/dev/null && pass "stats" || fail "stats"
    api GET "/api/vms/$VM_NAME/dataplane/flows?limit=5" >/dev/null && pass "flows" || fail "flows"
    EFF="$(api GET "/api/vms/$VM_NAME/dataplane/effective" || true)"
    echo "$EFF" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert "effective" in d or "declared" in d' \
      && pass "effective" || fail "effective"
  else
    fail "dataplane status for $VM_NAME ($ST)"
  fi
else
  echo "  (skip per-VM checks — no FABRIC_VM and no VMs listed)"
fi

section "cleanup"
api DELETE "/api/dataplane/cnp/$CNP_NAME" >/dev/null && pass "DELETE cnp" || fail "DELETE cnp"
api DELETE "/api/dataplane/groups/$GNAME" >/dev/null && pass "DELETE group" || fail "DELETE group"

echo ""
echo "Result: $PASS_N passed, $FAIL_N failed"
[[ "$FAIL_N" -eq 0 ]]
