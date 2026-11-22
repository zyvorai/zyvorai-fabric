#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0
# Verify /api/* and /api/v1/* return equivalent JSON for core VM routes.
set -euo pipefail

HOST="${1:-http://127.0.0.1:9095}"
USER="${FABRIC_USER:-admin}"
PASS="${FABRIC_PASS:-}"

if [[ -z "$PASS" ]]; then
  echo "Set FABRIC_PASS (or pass credentials via env)" >&2
  exit 1
fi

login() {
  curl -sf -X POST "${HOST}/api/auth/login" \
    -H 'Content-Type: application/json' \
    -d "{\"username\":\"${USER}\",\"password\":\"${PASS}\"}" \
    | python3 -c 'import sys,json; print(json.load(sys.stdin)["token"])'
}

TOKEN="$(login)"
AUTH=(-H "Authorization: Bearer ${TOKEN}")

fetch() {
  curl -sf "${AUTH[@]}" "${HOST}$1"
}

compare_paths() {
  local legacy="$1"
  local canonical="$2"
  local a b
  a="$(fetch "$legacy")"
  b="$(fetch "$canonical")"
  if [[ "$a" != "$b" ]]; then
    echo "MISMATCH: $legacy vs $canonical" >&2
    exit 1
  fi
  echo "OK $legacy == $canonical"
}

compare_paths "/api/v1/vms" "/api/vms"
compare_paths "/api/v1/capabilities" "/api/capabilities"
compare_paths "/api/v1/events" "/api/events"

echo "API prefix parity passed"
