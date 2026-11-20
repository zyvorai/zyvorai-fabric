#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0
#
# Lab post-deploy verify for Fabric:
#   devops contract + live gate + proven-infra + health/readyz + edge e2e.
#
# Auth for edge e2e (first match wins):
#   FABRIC_TOKEN / ZYVOR_FABRIC_TOKEN
#   FABRIC_PASSWORD / ZYVOR_FABRICD_ADMIN_PASSWORD
#   /var/lib/zyvor-fabricd/.admin_password (if readable)
#   lab default Admin@321 (when auth.db was seeded with FABRIC_LAB_DEFAULTS)
#
#   ./scripts/test-lab-verify.sh
#   RUN_CARGO=1 ./scripts/test-lab-verify.sh
#   FABRIC_URL=https://127.0.0.1:9095 FLUXVM_URL=http://127.0.0.1:7788 \
#     ./scripts/test-lab-verify.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/.."
cd "$ROOT"

# Keep stdin closed so nested tools never consume an outer ssh/heredoc pipe.
exec </dev/null

FABRIC_URL="${FABRIC_URL:-https://127.0.0.1:9095}"
FLUXVM_URL="${FLUXVM_URL:-http://127.0.0.1:7788}"
FABRIC_USER="${FABRIC_USER:-admin}"
export FABRIC_URL FLUXVM_URL FABRIC_USER

resolve_password() {
  if [[ -n "${FABRIC_PASSWORD:-}" ]]; then
    printf '%s' "$FABRIC_PASSWORD"
    return
  fi
  if [[ -n "${ZYVOR_FABRICD_ADMIN_PASSWORD:-}" ]]; then
    printf '%s' "$ZYVOR_FABRICD_ADMIN_PASSWORD"
    return
  fi
  local f="/var/lib/zyvor-fabricd/.admin_password"
  if [[ -r "$f" ]]; then
    tr -d '\n' <"$f"
    return
  fi
  if [[ -f "$f" ]] && command -v sudo >/dev/null 2>&1; then
    sudo cat "$f" 2>/dev/null | tr -d '\n' && return
  fi
  # Lab default used when FABRIC_LAB_DEFAULTS=1 seeded auth.db (file may be stale).
  printf '%s' 'Admin@321'
}

chmod +x scripts/test-proven-infra.sh scripts/test-edge-dataplane-e2e.sh \
  scripts/test-upgrade-rollback.sh scripts/upgrade-rollback.sh \
  scripts/chaos-qualify.sh scripts/devops-gate.sh scripts/test-devops-gate.sh \
  2>/dev/null || true

echo "########## Fabric: devops contract units ##########"
python3 -m unittest discover -s examples/devops -p 'test_*.py' -v

echo "########## Fabric: devops-gate (offline contract) ##########"
bash scripts/test-devops-gate.sh

echo "########## Fabric: devops-gate (live) ##########"
ZYVOR_ALLOW_OFFLINE=0 bash scripts/devops-gate.sh

echo "########## Fabric: proven-infra ##########"
./scripts/test-proven-infra.sh

echo "########## Fabric: health / readyz ##########"
H=$(curl -sk "$FABRIC_URL/health" || true)
R=$(curl -sk "$FABRIC_URL/readyz" || true)
echo "  health=$H"
echo "  readyz=$(echo "$R" | head -c 160)"
[[ "$H" == "OK" || "$H" == *"ok"* ]] || { echo "fabric /health failed" >&2; exit 1; }
echo "$R" | grep -qiE 'fluxvm|ok|ready' || { echo "fabric /readyz failed" >&2; exit 1; }
echo "  [PASS] fabric health+readyz"

echo "########## Fabric: edge dataplane e2e ##########"
if [[ -z "${FABRIC_TOKEN:-${ZYVOR_FABRIC_TOKEN:-}}" ]]; then
  export FABRIC_PASSWORD
  FABRIC_PASSWORD="$(resolve_password)"
fi
# If file password is stale vs auth.db, try lab default on login failure.
set +e
./scripts/test-edge-dataplane-e2e.sh
EC=$?
set -e
if [[ $EC -ne 0 && -z "${FABRIC_TOKEN:-}" ]]; then
  echo "  retry edge e2e with lab default Admin@321"
  FABRIC_PASSWORD='Admin@321' ./scripts/test-edge-dataplane-e2e.sh
fi

if [[ "${SKIP_WIREGUARD_SF:-}" != "1" ]]; then
  echo "########## Fabric: WireGuard + Service Fabric underlay ##########"
  chmod +x scripts/test-wireguard-service-fabric.sh 2>/dev/null || true
  ./scripts/test-wireguard-service-fabric.sh
fi

echo "########## Fabric lab verify: ALL GREEN ##########"
