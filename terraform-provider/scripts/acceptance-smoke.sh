#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0
# Smoke-test Terraform provider client methods against a running zyvor-fabricd.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$SCRIPT_DIR/.." && pwd)"
PROVIDER_DIR="$REPO/terraform-provider"

API_HOST="${API_HOST:-http://127.0.0.1:19095}"
ADMIN_PASS="${ZYVOR_FABRICD_ADMIN_PASSWORD:-ci-audit-password}"

if ! curl -sf "$API_HOST/health" >/dev/null 2>&1; then
  echo "zyvor-fabricd not reachable at $API_HOST — run scripts/ci-api-audit.sh setup or start daemon first" >&2
  exit 1
fi

TOKEN="$(curl -sf -X POST "$API_HOST/api/auth/login" \
  -H 'Content-Type: application/json' \
  -d "{\"username\":\"admin\",\"password\":\"$ADMIN_PASS\"}" \
  | python3 -c 'import sys,json; print(json.load(sys.stdin)["token"])')"

export ZYVOR_FABRICD_ENDPOINT="$API_HOST"
export ZYVOR_FABRICD_TOKEN="$TOKEN"

cd "$PROVIDER_DIR"
go test ./internal/provider/... -run TestNonExistent -count=0 >/dev/null
go build -o /tmp/terraform-provider-zyvor-fabricd .

# Client-level smoke via small Go program inline
go run ./tools/acceptance-smoke/main.go

echo "Terraform provider acceptance smoke passed"
