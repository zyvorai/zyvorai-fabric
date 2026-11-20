#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
GATE="$ROOT/scripts/devops-gate.sh"

# Offline without flag must fail
if FABRIC_URL=http://127.0.0.1:1 FLUXVM_URL=http://127.0.0.1:1 \
  ZYVOR_DEVOPS_TIMEOUT=0.2 ZYVOR_ALLOW_OFFLINE=0 \
  "$GATE"; then
  echo "expected fail when offline" >&2
  exit 1
fi

# Offline with flag must pass
FABRIC_URL=http://127.0.0.1:1 FLUXVM_URL=http://127.0.0.1:1 \
  ZYVOR_DEVOPS_TIMEOUT=0.2 ZYVOR_ALLOW_OFFLINE=1 \
  "$GATE"

# Optional live stack (self-signed HTTPS ok via curl -k)
if [[ "${ZYVOR_DEVOPS_LIVE:-0}" == "1" ]]; then
  unset FABRIC_URL
  FLUXVM_URL="${FLUXVM_URL:-http://127.0.0.1:7788}" \
    ZYVOR_ALLOW_OFFLINE=0 "$GATE"
fi

echo "test-devops-gate: ok"
