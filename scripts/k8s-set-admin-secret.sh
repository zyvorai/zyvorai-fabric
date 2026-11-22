#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0
# Create or update the Fabric Kubernetes credentials Secret (not systemd files).
#
# Password resolution (never silent Admin@321):
#   1. FABRIC_ADMIN_PASSWORD or ZYVOR_FABRICD_ADMIN_PASSWORD if set
#   2. else FABRIC_LAB_DEFAULTS=1 → Admin@321 (convenient lab only)
#   3. else generate a random password (openssl / python3)
#
# Usage:
#   ./scripts/k8s-set-admin-secret.sh                 # create if missing
#   ./scripts/k8s-set-admin-secret.sh --apply         # create or replace
#   FABRIC_LAB_DEFAULTS=1 ./scripts/k8s-set-admin-secret.sh --apply
#   FABRIC_ADMIN_PASSWORD='Secret!' ./scripts/k8s-set-admin-secret.sh --apply
#   ./scripts/k8s-set-admin-secret.sh --apply --restart
set -euo pipefail

NAMESPACE="${NAMESPACE:-zyvor-fabric}"
SECRET_NAME="${SECRET_NAME:-zyvor-fabric-secrets}"
ADMIN_USER="${FABRIC_ADMIN_USERNAME:-admin}"
APPLY=false
RESTART=false
GENERATED_PASS=false

for arg in "$@"; do
  case "$arg" in
    --apply) APPLY=true ;;
    --restart) RESTART=true ;;
    -h|--help)
      sed -n '2,16p' "$0" | sed 's/^# \?//'
      exit 0
      ;;
  esac
done

gen_random_password() {
  if command -v openssl >/dev/null 2>&1; then
    openssl rand -base64 18 2>/dev/null | tr -d '/+=' | head -c 24
    return 0
  fi
  if command -v python3 >/dev/null 2>&1; then
    python3 -c 'import secrets; print(secrets.token_urlsafe(18)[:24])'
    return 0
  fi
  echo "error: need openssl or python3 to generate admin password (or set FABRIC_ADMIN_PASSWORD / FABRIC_LAB_DEFAULTS=1)" >&2
  exit 1
}

if [[ -n "${FABRIC_ADMIN_PASSWORD:-}" ]]; then
  ADMIN_PASS="${FABRIC_ADMIN_PASSWORD}"
elif [[ -n "${ZYVOR_FABRICD_ADMIN_PASSWORD:-}" ]]; then
  ADMIN_PASS="${ZYVOR_FABRICD_ADMIN_PASSWORD}"
elif [[ "${FABRIC_LAB_DEFAULTS:-}" == "1" ]]; then
  ADMIN_PASS='Admin@321'
else
  ADMIN_PASS="$(gen_random_password)"
  GENERATED_PASS=true
fi

if kubectl get secret "${SECRET_NAME}" -n "${NAMESPACE}" &>/dev/null && ! $APPLY; then
  echo "Secret ${SECRET_NAME} already exists in ${NAMESPACE} (pass --apply to replace)."
  exit 0
fi

JWT_SECRET="${FABRIC_JWT_SECRET:-$(openssl rand -base64 32 2>/dev/null || head -c 32 /dev/urandom | base64)}"

kubectl create namespace "${NAMESPACE}" --dry-run=client -o yaml | kubectl apply -f - >/dev/null

kubectl create secret generic "${SECRET_NAME}" \
  --from-literal=admin-username="${ADMIN_USER}" \
  --from-literal=admin-password="${ADMIN_PASS}" \
  --from-literal=jwt-secret="${JWT_SECRET}" \
  -n "${NAMESPACE}" \
  --dry-run=client -o yaml | kubectl apply -f -

echo "Applied Secret/${SECRET_NAME} in ${NAMESPACE}"
echo "  admin-username: ${ADMIN_USER}"
if $GENERATED_PASS; then
  echo "  admin-password: (generated — not printed)"
  echo "Retrieve:"
  echo "  kubectl -n ${NAMESPACE} get secret ${SECRET_NAME} -o jsonpath='{.data.admin-password}' | base64 -d; echo"
elif [[ "${FABRIC_LAB_DEFAULTS:-}" == "1" ]] && [[ -z "${FABRIC_ADMIN_PASSWORD:-}" ]] && [[ -z "${ZYVOR_FABRICD_ADMIN_PASSWORD:-}" ]]; then
  echo "  admin-password: Admin@321 (FABRIC_LAB_DEFAULTS=1)"
else
  echo "  admin-password: (from FABRIC_ADMIN_PASSWORD / ZYVOR_FABRICD_ADMIN_PASSWORD)"
fi

if $RESTART; then
  kubectl -n "${NAMESPACE}" rollout restart daemonset/zyvor-fabricd 2>/dev/null || true
  echo "Restarted daemonset/zyvor-fabricd (password seed only applies when auth.db is empty)."
fi
