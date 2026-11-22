#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0
# Convenience wrapper for K8s remote deploy (Ragnarok-style).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/deploy-common.sh
source "${SCRIPT_DIR}/lib/deploy-common.sh"

HOST="${1:-${DEPLOY_HOST:-}}"
USER="${2:-${DEPLOY_USER:-}}"

if [ $# -eq 0 ] && [ -z "${HOST}" ]; then
  echo "Usage: ./scripts/deploy-k8s-remote.sh <host> [user] [--quick|--uninstall]"
  echo "   or: ./scripts/deploy k8s USER@HOST [--quick]"
  exit 1
fi

deploy_ui_banner "Fabric K8s remote → ${HOST:-?}" "${USER:-}"
exec "${SCRIPT_DIR}/deploy-k8s-all-remote.sh" "$@"
