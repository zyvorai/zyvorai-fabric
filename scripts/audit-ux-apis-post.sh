#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0
# POST smoke tests for sandbox CRUD objects (network policy scaffold).
set -euo pipefail

HOST="${1:-http://127.0.0.1:9095}"
USER="${FABRIC_USER:-admin}"
PASS="${FABRIC_PASS:-max}"

login() {
  curl -sf -X POST "${HOST}/api/auth/login" \
    -H 'Content-Type: application/json' \
    -d "{\"username\":\"${USER}\",\"password\":\"${PASS}\"}" \
    | python3 -c 'import sys,json; print(json.load(sys.stdin)["token"])'
}

TOKEN="$(login)"
AUTH=(-H "Authorization: Bearer ${TOKEN}" -H "Content-Type: application/json")

POLICY_ID="$(curl -sf "${AUTH[@]}" -X POST "${HOST}/api/network-policies" \
  -d '{"name":"audit-smoke-policy","description":"ci post smoke","endpoint_selector":{"match_labels":{}},"ingress":[],"egress":[],"enabled":true}' \
  | python3 -c 'import sys,json; print(json.load(sys.stdin)["id"])')"

curl -sf "${AUTH[@]}" -X DELETE "${HOST}/api/network-policies/${POLICY_ID}" >/dev/null

# Retention config round-trip
curl -sf "${AUTH[@]}" -X PUT "${HOST}/api/events/retention" -d '{"max_events":1500}' >/dev/null
curl -sf "${AUTH[@]}" "${HOST}/api/events/retention" | python3 -c 'import sys,json; assert json.load(sys.stdin)["max_events"]==1500'

# Config snapshot export
curl -sf "${AUTH[@]}" "${HOST}/api/config/snapshot" | python3 -c 'import sys,json; d=json.load(sys.stdin); assert "exported_at" in d'

echo "POST smoke tests passed"
