#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0
# CI smoke: build zyvor-fabricd, start on localhost, run audit-ux-apis.sh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$SCRIPT_DIR/.." && pwd)"
BACKEND="$REPO/backend"
WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/zyvor-fabricd-ci.XXXXXX")"
BIN="$BACKEND/target/release/zyvor-fabricd"
API_HOST="http://127.0.0.1:19095"
API_PORT="19095"
ADMIN_PASS="${ZYVOR_FABRICD_ADMIN_PASSWORD:-ci-audit-password}"
JWT_SECRET="${ZYVOR_FABRICD_JWT_SECRET:-ci-audit-jwt-secret-for-github-actions}"

cleanup() {
  if [[ -n "${DAEMON_PID:-}" ]] && kill -0 "$DAEMON_PID" 2>/dev/null; then
    kill "$DAEMON_PID" 2>/dev/null || true
    wait "$DAEMON_PID" 2>/dev/null || true
  fi
  rm -rf "$WORKDIR"
}
trap cleanup EXIT

mkdir -p "$WORKDIR/data/images"

cat > "$WORKDIR/zyvor-fabricd.toml" <<EOF
[daemon]
listen = "127.0.0.1:${API_PORT}"

[storage]
path = "$WORKDIR/data"
image_path = "$WORKDIR/data/images"

[network]
bridge = "br0"

[auth]
enabled = true
jwt_secret = "$JWT_SECRET"
db_path = "$WORKDIR/data/auth.db"
default_admin_password = "$ADMIN_PASS"

# This script only ever talks to $API_HOST over plain HTTP, so there's no
# reason to generate a self-signed cert -- and TlsConfig's own defaults
# point at root-owned /etc/zyvor-fabricd/tls, which a CI runner can't write
# to. TLS has no env-var override, only this file, so it must be set here.
[tls]
enabled = false
EOF

echo "==> Building zyvor-fabricd (release)..."
(cd "$BACKEND" && cargo build --release -p zyvor-fabricd)

echo "==> Starting zyvor-fabricd (config: $WORKDIR/zyvor-fabricd.toml)..."
export ZYVOR_FABRICD_CONFIG="$WORKDIR/zyvor-fabricd.toml"
export ZYVOR_FABRICD_ADMIN_PASSWORD="$ADMIN_PASS"
export ZYVOR_FABRICD_JWT_SECRET="$JWT_SECRET"
"$BIN" >"$WORKDIR/zyvor-fabricd.log" 2>&1 &
DAEMON_PID=$!

ready=0
for _ in $(seq 1 60); do
  if curl -sf "$API_HOST/health" >/dev/null 2>&1; then
    ready=1
    break
  fi
  if ! kill -0 "$DAEMON_PID" 2>/dev/null; then
    echo "zyvor-fabricd exited early:" >&2
    cat "$WORKDIR/zyvor-fabricd.log" >&2 || true
    exit 1
  fi
  sleep 0.5
done

if [[ "$ready" -ne 1 ]]; then
  echo "zyvor-fabricd failed to become ready:" >&2
  cat "$WORKDIR/zyvor-fabricd.log" >&2 || true
  exit 1
fi

echo "==> Running UX API audit..."
FABRIC_USER=admin FABRIC_PASS="$ADMIN_PASS" "$SCRIPT_DIR/audit-ux-apis.sh" "$API_HOST"

echo "==> POST smoke tests..."
FABRIC_USER=admin FABRIC_PASS="$ADMIN_PASS" "$SCRIPT_DIR/audit-ux-apis-post.sh" "$API_HOST"

echo "==> API prefix parity (/api vs /api/v1)..."
FABRIC_USER=admin FABRIC_PASS="$ADMIN_PASS" "$SCRIPT_DIR/test-api-prefix-parity.sh" "$API_HOST"

echo "==> OpenAPI tier-1 coverage..."
python3 "$SCRIPT_DIR/check-openapi-coverage.py"

echo "==> CI API audit passed"
