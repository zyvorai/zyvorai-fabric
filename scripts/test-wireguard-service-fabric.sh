#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: Apache-2.0
#
# WireGuard VPN Mesh underlay + Service Fabric remote-backends / remote-identities.
#
# Single-host loopback P2P (two Fabric tunnels, endpoints on 127.0.0.1) then:
#   1. handshake + ping over AllowedIPs
#   2. Maglev service + remote-backend reconcile (peer over WG)
#   3. remote-identity reconcile → FluxVM ipcache
#   4. idempotent POST /api/vpn-tunnels/sync (must be 200)
#   5. VpnTab-shaped API list/status smoke
#
# Env:
#   FABRIC_URL=https://127.0.0.1:9095
#   FLUXVM_URL=http://127.0.0.1:7788
#   FABRIC_TOKEN=… | FABRIC_USER/FABRIC_PASSWORD (default admin / Admin@321)
#   MERION_LAB=1          leave lab tunnels/services (default: cleanup)
#   SKIP_FLUXVM=1       skip Maglev / ipcache checks
#
#   ./scripts/test-wireguard-service-fabric.sh
# Pre-rename names still work: KEEP_<X> is used when MERION_<X> is unset.
[ -n "${MERION_LAB+x}" ] || [ -z "${KEEP_LAB:-}" ] || export MERION_LAB="$KEEP_LAB"

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
exec </dev/null

FABRIC_URL="${FABRIC_URL:-https://127.0.0.1:9095}"
FLUXVM_URL="${FLUXVM_URL:-http://127.0.0.1:7788}"
FABRIC_USER="${FABRIC_USER:-admin}"
FABRIC_PASSWORD="${FABRIC_PASSWORD:-Admin@321}"
ROUTE_DOMAIN="${ROUTE_DOMAIN:-lab-wg}"
VIP="${VIP:-10.96.77.10}"
SNAT="${SNAT:-10.96.77.1}"
SERVICE="${SERVICE:-wg-mesh-demo}"
IF_A="${IF_A:-wg-lab-a}"
IF_B="${IF_B:-wg-lab-b}"
PORT_A="${PORT_A:-51821}"
PORT_B="${PORT_B:-51822}"
ADDR_A="${ADDR_A:-10.66.77.1/24}"
ADDR_B="${ADDR_B:-10.66.77.2/24}"
PEER_A_IP="${PEER_A_IP:-10.66.77.1}"
PEER_B_IP="${PEER_B_IP:-10.66.77.2}"
IDENTITY_ID="${IDENTITY_ID:-424242}"

fail() { echo "FAIL: $*" >&2; exit 1; }
pass() { echo "PASS: $*"; }
need() { command -v "$1" >/dev/null 2>&1 || fail "missing dependency: $1"; }

need curl
need python3
need wg

login() {
  if [[ -n "${FABRIC_TOKEN:-}" ]]; then
    printf '%s' "$FABRIC_TOKEN"
    return
  fi
  curl -sk -X POST "$FABRIC_URL/api/auth/login" \
    -H "Content-Type: application/json" \
    -d "{\"username\":\"$FABRIC_USER\",\"password\":\"$FABRIC_PASSWORD\"}" \
    | python3 -c 'import sys,json; print(json.load(sys.stdin)["token"])'
}

TOKEN="$(login)"
[[ -n "$TOKEN" ]] || fail "fabric login failed"
AUTH=("Authorization: Bearer $TOKEN")
CT="Content-Type: application/json"

api() {
  local method="$1" path="$2"
  shift 2
  curl -sk -X "$method" -H "${AUTH[0]}" -H "$CT" "$@" "${FABRIC_URL}${path}"
}

echo "########## WireGuard + Service Fabric underlay ##########"
echo "  FABRIC_URL=$FABRIC_URL FLUXVM_URL=$FLUXVM_URL"

# Delete prior lab tunnels
python3 - <<PY
import json, os, urllib.request, ssl
ctx = ssl._create_unverified_context()
token = os.environ.get("TOKEN") or """$TOKEN"""
req = urllib.request.Request(
    "$FABRIC_URL/api/vpn-tunnels",
    headers={"Authorization": f"Bearer {token}"},
)
with urllib.request.urlopen(req, context=ctx) as r:
    tunnels = json.load(r)
for t in tunnels:
    name = t.get("name", "")
    iface = t.get("interface_name", "")
    if name.startswith("lab-") or iface in ("$IF_A", "$IF_B"):
        d = urllib.request.Request(
            f"$FABRIC_URL/api/vpn-tunnels/{t['id']}",
            method="DELETE",
            headers={"Authorization": f"Bearer {token}"},
        )
        try:
            urllib.request.urlopen(d, context=ctx)
            print(f"  cleaned tunnel {name}")
        except Exception as e:
            print(f"  cleanup warn {name}: {e}")
PY

PRIV_A="$(wg genkey)"
PUB_A="$(printf '%s' "$PRIV_A" | wg pubkey)"
PRIV_B="$(wg genkey)"
PUB_B="$(printf '%s' "$PRIV_B" | wg pubkey)"

BODY_A="$(PRIV_A="$PRIV_A" PUB_B="$PUB_B" IF_A="$IF_A" PORT_A="$PORT_A" ADDR_A="$ADDR_A" PORT_B="$PORT_B" PEER_B_IP="$PEER_B_IP" python3 - <<'PY'
import json, os
print(json.dumps({
  "name": "lab-a",
  "interface_name": os.environ["IF_A"],
  "listen_port": int(os.environ["PORT_A"]),
  "address": os.environ["ADDR_A"],
  "private_key_ref": os.environ["PRIV_A"],
  "enabled": True,
  "peers": [{
    "public_key": os.environ["PUB_B"],
    "endpoint": f"127.0.0.1:{os.environ['PORT_B']}",
    "allowed_ips": [f"{os.environ['PEER_B_IP']}/32", "10.88.77.0/24"],
    "persistent_keepalive": 25,
  }],
}))
PY
)"

BODY_B="$(PRIV_B="$PRIV_B" PUB_A="$PUB_A" IF_B="$IF_B" PORT_B="$PORT_B" ADDR_B="$ADDR_B" PORT_A="$PORT_A" PEER_A_IP="$PEER_A_IP" python3 - <<'PY'
import json, os
print(json.dumps({
  "name": "lab-b",
  "interface_name": os.environ["IF_B"],
  "listen_port": int(os.environ["PORT_B"]),
  "address": os.environ["ADDR_B"],
  "private_key_ref": os.environ["PRIV_B"],
  "enabled": True,
  "peers": [{
    "public_key": os.environ["PUB_A"],
    "endpoint": f"127.0.0.1:{os.environ['PORT_A']}",
    "allowed_ips": [f"{os.environ['PEER_A_IP']}/32", "10.88.76.0/24"],
    "persistent_keepalive": 25,
  }],
}))
PY
)"

echo "== create tunnels =="
code_a="$(api POST /api/vpn-tunnels -d "$BODY_A" -o /tmp/wg-sf-a.json -w '%{http_code}')"
code_b="$(api POST /api/vpn-tunnels -d "$BODY_B" -o /tmp/wg-sf-b.json -w '%{http_code}')"
[[ "$code_a" == "201" ]] || fail "create lab-a HTTP $code_a $(head -c 200 /tmp/wg-sf-a.json)"
[[ "$code_b" == "201" ]] || fail "create lab-b HTTP $code_b $(head -c 200 /tmp/wg-sf-b.json)"
pass "created lab-a + lab-b"

echo "== sync (idempotent) =="
sync1="$(api POST /api/vpn-tunnels/sync -o /tmp/wg-sf-sync.json -w '%{http_code}')"
[[ "$sync1" == "200" ]] || fail "sync #1 HTTP $sync1 $(cat /tmp/wg-sf-sync.json)"
sync2="$(api POST /api/vpn-tunnels/sync -o /tmp/wg-sf-sync2.json -w '%{http_code}')"
[[ "$sync2" == "200" ]] || fail "sync #2 (idempotent) HTTP $sync2 $(cat /tmp/wg-sf-sync2.json)"
pass "vpn-tunnels/sync idempotent"

echo "== handshake + ping =="
sudo wg show "$IF_A" | tee /tmp/wg-sf-show-a.txt | grep -qi 'latest handshake\|transfer:' \
  || fail "no handshake/transfer on $IF_A"
sudo wg show "$IF_B" | tee /tmp/wg-sf-show-b.txt | grep -qi 'latest handshake\|transfer:' \
  || fail "no handshake/transfer on $IF_B"
ping -c 2 -W 2 "$PEER_B_IP" >/dev/null || fail "ping $PEER_B_IP over WG"
ping -c 2 -W 2 "$PEER_A_IP" >/dev/null || fail "ping $PEER_A_IP over WG"
pass "handshake + AllowedIPs reachability"

echo "== VpnTab API smoke =="
api GET /api/vpn-tunnels -o /tmp/wg-sf-list.json >/dev/null
api GET /api/vpn-tunnels/status -o /tmp/wg-sf-status.json >/dev/null
python3 - <<'PY'
import json
t = json.load(open("/tmp/wg-sf-list.json"))
s = json.load(open("/tmp/wg-sf-status.json"))
names = {x.get("name") for x in t}
assert "lab-a" in names and "lab-b" in names, names
assert isinstance(s, list) and len(s) >= 2, s
print(f"  tunnels={len(t)} status_rows={len(s)}")
PY
pass "console VPN APIs"

if [[ "${SKIP_FLUXVM:-}" == "1" ]]; then
  pass "SKIP_FLUXVM=1 — stopping after VPN Mesh"
  exit 0
fi

echo "== Maglev service + remote-backend over WG =="
# Ensure VIP aliases exist on a NS iface when present (best-effort).
if ip link show sf-lab0 >/dev/null 2>&1; then
  sudo ip addr add "${VIP}/32" dev sf-lab0 2>/dev/null || true
  sudo ip addr add "${SNAT}/32" dev sf-lab0 2>/dev/null || true
fi

api POST /api/dataplane/services -d "$(python3 - <<PY
import json
print(json.dumps({
  "name": "$SERVICE",
  "vip": "$VIP",
  "port": 8080,
  "protocol": "tcp",
  "backends": [{"address": "$PEER_A_IP", "port": 8080, "weight": 1}],
  "site_id": "site-a",
  "route_domain": "$ROUTE_DOMAIN",
  "exposure": "north-south",
  "snat_address": "$SNAT",
  "mode": "nat",
}))
PY
)" -o /tmp/wg-sf-svc.json -w '\nHTTP:%{http_code}\n' | tee /tmp/wg-sf-svc.http
grep -q 'HTTP:200' /tmp/wg-sf-svc.http || fail "service upsert $(cat /tmp/wg-sf-svc.json)"

api POST /api/dataplane/remote-backends -d "$(python3 - <<PY
import json
print(json.dumps({
  "service": "$SERVICE",
  "site_id": "site-b",
  "route_domain": "$ROUTE_DOMAIN",
  "vip": "$VIP",
  "address": "$PEER_B_IP",
  "port": 8080,
  "weight": 2,
  "state": "ready",
  "labels": {"via": "wireguard", "peer": "$IF_B"},
}))
PY
)" -o /tmp/wg-sf-rb.json -w '\nHTTP:%{http_code}\n' | tee /tmp/wg-sf-rb.http
grep -q 'HTTP:200' /tmp/wg-sf-rb.http || fail "remote-backend upsert"

api POST /api/dataplane/remote-backends/reconcile \
  -d "{\"route_domain\":\"$ROUTE_DOMAIN\"}" \
  -o /tmp/wg-sf-rb-rec.json -w '\nHTTP:%{http_code}\n' | tee /tmp/wg-sf-rb-rec.http
grep -q 'HTTP:200' /tmp/wg-sf-rb-rec.http || fail "remote-backend reconcile"
python3 - <<PY
import json
r = json.load(open("/tmp/wg-sf-rb-rec.json"))
assert r.get("remote_backends", 0) >= 1, r
assert "$SERVICE" in (r.get("services") or []), r
print("  reconcile", r)
PY

curl -sf "$FLUXVM_URL/v1/network/services/$SERVICE" -o /tmp/wg-sf-maglev.json \
  || fail "FluxVM missing service $SERVICE"
python3 - <<PY
import json
s = json.load(open("/tmp/wg-sf-maglev.json"))
addrs = {(b.get("address"), b.get("port")) for b in s.get("backends") or []}
assert ("$PEER_A_IP", 8080) in addrs, addrs
assert ("$PEER_B_IP", 8080) in addrs, addrs
print("  maglev backends", sorted(addrs))
PY
pass "remote-backends merged over WG AllowedIPs"

echo "== remote-identity fan-out =="
api POST /api/dataplane/remote-identities -d "$(python3 - <<PY
import json
print(json.dumps({
  "identity_id": int("$IDENTITY_ID"),
  "site_id": "site-b",
  "route_domain": "$ROUTE_DOMAIN",
  "cidrs": ["$PEER_B_IP/32", "10.88.77.0/24"],
  "labels": {"via": "wireguard"},
}))
PY
)" -o /tmp/wg-sf-ri.json -w '\nHTTP:%{http_code}\n' | tee /tmp/wg-sf-ri.http
grep -q 'HTTP:200' /tmp/wg-sf-ri.http || fail "remote-identity upsert"

api POST /api/dataplane/remote-identities/reconcile \
  -d "{\"route_domain\":\"$ROUTE_DOMAIN\"}" \
  -o /tmp/wg-sf-ri-rec.json -w '\nHTTP:%{http_code}\n' | tee /tmp/wg-sf-ri-rec.http
grep -q 'HTTP:200' /tmp/wg-sf-ri-rec.http || fail "remote-identity reconcile"
python3 - <<PY
import json
r = json.load(open("/tmp/wg-sf-ri-rec.json"))
ids = r.get("identities") or []
assert int("$IDENTITY_ID") in ids, r
print("  identities", ids)
PY

curl -sf "$FLUXVM_URL/v1/network/ipcache" -o /tmp/wg-sf-ipcache.json \
  || fail "FluxVM ipcache list failed"
python3 - <<PY
import json
doc = json.load(open("/tmp/wg-sf-ipcache.json"))
items = doc.get("items") if isinstance(doc, dict) else doc
found = [x for x in items if x.get("identity") == int("$IDENTITY_ID") and x.get("ip") == "$PEER_B_IP"]
assert found, items[:20]
print("  ipcache", found[0])
PY
pass "remote-identities → FluxVM ipcache"

if [[ "${MERION_LAB:-}" != "1" ]]; then
  echo "== cleanup =="
  api DELETE "/api/dataplane/remote-backends/$ROUTE_DOMAIN/$SERVICE/$PEER_B_IP/8080" -o /dev/null -w '' || true
  api DELETE "/api/dataplane/remote-identities/$ROUTE_DOMAIN/$IDENTITY_ID" -o /dev/null -w '' || true
  curl -sf -X DELETE "$FLUXVM_URL/v1/network/services/$SERVICE" >/dev/null 2>&1 || true
  python3 - <<PY
import json, urllib.request, ssl
ctx = ssl._create_unverified_context()
token = """$TOKEN"""
req = urllib.request.Request(
    "$FABRIC_URL/api/vpn-tunnels",
    headers={"Authorization": f"Bearer {token}"},
)
with urllib.request.urlopen(req, context=ctx) as r:
    tunnels = json.load(r)
for t in tunnels:
    if t.get("name", "").startswith("lab-"):
        d = urllib.request.Request(
            f"$FABRIC_URL/api/vpn-tunnels/{t['id']}",
            method="DELETE",
            headers={"Authorization": f"Bearer {token}"},
        )
        try:
            urllib.request.urlopen(d, context=ctx)
        except Exception:
            pass
PY
  api POST /api/vpn-tunnels/sync -o /dev/null -w '' || true
fi

echo "########## ALL PASS: WireGuard + Service Fabric underlay ##########"
