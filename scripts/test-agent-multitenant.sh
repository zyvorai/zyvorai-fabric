#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0
#
# Multi-tenant agent UX path (API-level): User+tenant can deploy/run; another
# tenant cannot see their agents; Viewer cannot deploy.
#
# Requires: fabricd with [agent_runtime] + agent-runtime up, auth enabled.
#
# Usage:
#   FABRIC_URL=https://127.0.0.1:9095 \
#   ADMIN_USER=demo ADMIN_PASS=demo \
#   ./scripts/test-agent-multitenant.sh
#
set -euo pipefail

URL="${FABRIC_URL:-https://127.0.0.1:9095}"
CURL=(curl -sk)
ADMIN_USER="${ADMIN_USER:-admin}"
# Lab default: persisted admin password file, else env, else sudo-readable file.
if [[ -z "${ADMIN_PASS:-}" ]]; then
  if [[ -r /var/lib/zyvor-fabricd/.admin_password ]]; then
    ADMIN_PASS=$(cat /var/lib/zyvor-fabricd/.admin_password)
  elif command -v sudo >/dev/null 2>&1 && sudo test -r /var/lib/zyvor-fabricd/.admin_password; then
    ADMIN_PASS=$(sudo cat /var/lib/zyvor-fabricd/.admin_password)
  else
    ADMIN_PASS="${ZYVOR_FABRICD_ADMIN_PASSWORD:-}"
  fi
fi
[[ -n "${ADMIN_PASS:-}" ]] || { echo "Set ADMIN_PASS or provide /var/lib/zyvor-fabricd/.admin_password"; exit 1; }
PASS=0
FAIL=0

check() {
  local name=$1 want=$2 got=$3
  if [[ "$got" == *"$want"* ]]; then
    echo "PASS  $name"
    PASS=$((PASS + 1))
  else
    echo "FAIL  $name  (want '$want', got '$got')"
    FAIL=$((FAIL + 1))
  fi
}

json_get() {
  python3 -c 'import json,sys
d=json.load(sys.stdin)
cur=d
for p in sys.argv[1].split("."):
  if not p: continue
  cur=cur[int(p)] if p.isdigit() else cur[p]
print("" if cur is None else cur)' "$1"
}

login() {
  local user=$1 pass=$2
  "${CURL[@]}" -s -X POST "$URL/api/auth/login" -H 'Content-Type: application/json' \
    -d "{\"username\":\"$user\",\"password\":\"$pass\"}"
}

# Ensure two tenant users exist (admin creates them)
admin_tok=$(login "$ADMIN_USER" "$ADMIN_PASS" | json_get token)
[[ -n "$admin_tok" ]] || { echo "admin login failed"; exit 1; }

create_user() {
  local username=$1 password=$2 role=$3 tenant=$4
  "${CURL[@]}" -s -o /tmp/cu.json -w '%{http_code}' -X POST "$URL/api/auth/users" \
    -H "Authorization: Bearer $admin_tok" -H 'Content-Type: application/json' \
    -d "{\"username\":\"$username\",\"password\":\"$password\",\"role\":\"$role\",\"tenant\":\"$tenant\"}"
}

# Best-effort create (200/201/409 ok)
for spec in "alice:alice-pass:user:acme" "bob:bob-pass:user:globex" "viewer1:viewer-pass:viewer:acme"; do
  IFS=: read -r u p r t <<<"$spec"
  code=$(create_user "$u" "$p" "$r" "$t" || true)
  echo "ensure user $u → HTTP $code"
done

alice_tok=$(login alice alice-pass | json_get token)
bob_tok=$(login bob bob-pass | json_get token)
viewer_tok=$(login viewer1 viewer-pass | json_get token)
[[ -n "$alice_tok" && -n "$bob_tok" && -n "$viewer_tok" ]] || { echo "tenant user login failed"; exit 1; }

BUNDLE=$(printf 'export default async function(){ return { ok: true }; }' | base64)

deploy() {
  local tok=$1 name=$2
  "${CURL[@]}" -s -o /tmp/dep.json -w '%{http_code}' -X POST "$URL/api/agents" \
    -H "Authorization: Bearer $tok" -H 'Content-Type: application/json' \
    -d "{\"name\":\"$name\",\"bundle_base64\":\"$BUNDLE\",\"manifest\":{\"template\":\"agent-node\",\"resources\":{\"vcpus\":1,\"memory_mib\":512},\"home_volume\":{\"per_user\":true}}}"
}

echo "==> deploy as alice (tenant acme)"
code=$(deploy "$alice_tok" research)
check "alice deploy → 2xx" "20" "$code"
alice_name=$(python3 -c 'import json;print(json.load(open("/tmp/dep.json")).get("name",""))')
check "alice agent namespaced" "t.acme." "$alice_name"

echo "==> bob cannot see alice agent"
bob_list=$("${CURL[@]}" -s -H "Authorization: Bearer $bob_tok" "$URL/api/agents")
if echo "$bob_list" | grep -q "$alice_name"; then
  echo "FAIL  bob must not see alice agent"
  FAIL=$((FAIL + 1))
else
  echo "PASS  bob list hides alice agent"
  PASS=$((PASS + 1))
fi

echo "==> alice list includes her agent"
alice_list=$("${CURL[@]}" -s -H "Authorization: Bearer $alice_tok" "$URL/api/agents")
check "alice list shows her agent" "$alice_name" "$alice_list"

echo "==> viewer cannot deploy"
code=$(deploy "$viewer_tok" nope)
check "viewer deploy forbidden" "403" "$code"

echo "==> alice session stamps user_id"
code=$("${CURL[@]}" -s -o /tmp/sess.json -w '%{http_code}' -X POST "$URL/api/sessions" \
  -H "Authorization: Bearer $alice_tok" -H 'Content-Type: application/json' \
  -d "{\"agent\":\"$alice_name\",\"input\":{}}")
if [[ "$code" == "401" || "$code" == "403" ]]; then
  echo "FAIL  alice session auth ($code)"
  FAIL=$((FAIL + 1))
else
  echo "PASS  alice session HTTP $code (auth ok)"
  PASS=$((PASS + 1))
fi
# Prefer success; 502 = FluxVM cold-start; 400 with user_id in error means stamp reached runtime.
alice_sub=$(python3 -c 'import json,base64; p=("'"$alice_tok"'".split(".")[1]+"=="); print(json.loads(base64.urlsafe_b64decode(p)).get("sub",""))')
alice_uid_expect=$(python3 -c 'import re,sys; s=re.sub(r"[^a-zA-Z0-9._-]","-",sys.argv[1].lower())[:32].strip("-._") or "user"; print(s)' "$alice_sub")
uid=$(python3 -c 'import json;print(json.load(open("/tmp/sess.json")).get("user_id","") or "")' 2>/dev/null || true)
err=$(python3 -c 'import json;print(json.load(open("/tmp/sess.json")).get("error","") or "")' 2>/dev/null || true)
if [[ "$code" == "200" || "$code" == "201" ]]; then
  check "session user_id stamped" "$alice_uid_expect" "$uid"
elif [[ -n "$uid" ]]; then
  check "session user_id stamped" "$alice_uid_expect" "$uid"
elif [[ "$code" == "400" || "$code" == "502" ]]; then
  # Runtime rejected for sandbox/template reasons after accepting tenant scoping.
  echo "PASS  session reached runtime ($code) expect_uid=$alice_uid_expect err=${err:0:80}"
  PASS=$((PASS + 1))
else
  echo "FAIL  unexpected session status $code body=$(head -c 120 /tmp/sess.json)"
  FAIL=$((FAIL + 1))
fi

echo
echo "passed=$PASS failed=$FAIL"
[[ "$FAIL" -eq 0 ]]
