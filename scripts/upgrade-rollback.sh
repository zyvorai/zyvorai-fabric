#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0
# N→N+1 snapshot / verify / rollback for zyvor-fabricd state + config.
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  upgrade-rollback.sh snapshot  [--tag TAG] [--state-dir DIR] [--config-dir DIR] [--backup-root DIR]
  upgrade-rollback.sh rollback  [--tag TAG] [--state-dir DIR] [--config-dir DIR] [--backup-root DIR]
  upgrade-rollback.sh verify    [--base-url URL] [--timeout SEC]
  upgrade-rollback.sh list      [--backup-root DIR]

Defaults:
  state-dir    /var/lib/zyvor-fabricd
  config-dir   /etc/zyvor-fabricd
  backup-root  /var/lib/zyvor-fabricd/upgrade-snapshots
  base-url     http://127.0.0.1:9095
EOF
}

cmd="${1:-}"
shift || true

TAG=""
STATE_DIR="${ZYVOR_STATE_DIR:-/var/lib/zyvor-fabricd}"
CONFIG_DIR="${ZYVOR_CONFIG_DIR:-/etc/zyvor-fabricd}"
BACKUP_ROOT="${ZYVOR_UPGRADE_BACKUP_ROOT:-/var/lib/zyvor-fabricd/upgrade-snapshots}"
BASE_URL="${FABRIC_URL:-http://127.0.0.1:9095}"
TIMEOUT="${ZYVOR_VERIFY_TIMEOUT:-2}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --tag) TAG="$2"; shift 2 ;;
    --state-dir) STATE_DIR="$2"; shift 2 ;;
    --config-dir) CONFIG_DIR="$2"; shift 2 ;;
    --backup-root) BACKUP_ROOT="$2"; shift 2 ;;
    --base-url) BASE_URL="$2"; shift 2 ;;
    --timeout) TIMEOUT="$2"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown arg: $1" >&2; usage; exit 2 ;;
  esac
done

need_tag() {
  if [[ -z "$TAG" ]]; then
    echo "error: --tag is required" >&2
    exit 2
  fi
  if [[ ! "$TAG" =~ ^[A-Za-z0-9._-]+$ ]]; then
    echo "error: invalid tag" >&2
    exit 2
  fi
}

snap_dir() {
  printf '%s/%s\n' "$BACKUP_ROOT" "$TAG"
}

do_snapshot() {
  need_tag
  local dest
  dest="$(snap_dir)"
  mkdir -p "$dest/config"
  mkdir -p "$STATE_DIR" "$CONFIG_DIR"
  if command -v tar >/dev/null 2>&1; then
    tar -C "$(dirname "$STATE_DIR")" -czf "$dest/state.tar.gz" "$(basename "$STATE_DIR")"
  else
    echo "error: tar required" >&2
    exit 1
  fi
  if [[ -d "$CONFIG_DIR" ]]; then
    cp -a "$CONFIG_DIR"/. "$dest/config/" 2>/dev/null || true
  fi
  # Preserve secret modes if present
  for secret in .jwt_secret .admin_password; do
    if [[ -f "$STATE_DIR/$secret" ]]; then
      cp -a "$STATE_DIR/$secret" "$dest/$secret"
      chmod 0600 "$dest/$secret" || true
    fi
  done
  {
    echo "tag=$TAG"
    echo "created=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "state_dir=$STATE_DIR"
    echo "config_dir=$CONFIG_DIR"
    echo "fabric_version=${ZYVOR_FABRIC_VERSION:-unknown}"
    echo "fluxvm_version=${FLUXVM_VERSION:-unknown}"
  } >"$dest/MANIFEST"
  echo "snapshot written: $dest"
}

do_rollback() {
  need_tag
  local dest
  dest="$(snap_dir)"
  if [[ ! -f "$dest/state.tar.gz" || ! -f "$dest/MANIFEST" ]]; then
    echo "error: snapshot $TAG not found under $BACKUP_ROOT" >&2
    exit 1
  fi
  mkdir -p "$(dirname "$STATE_DIR")" "$CONFIG_DIR"
  # Replace state tree from archive
  local parent
  parent="$(dirname "$STATE_DIR")"
  tar -C "$parent" -xzf "$dest/state.tar.gz"
  if [[ -d "$dest/config" ]]; then
    mkdir -p "$CONFIG_DIR"
    cp -a "$dest/config"/. "$CONFIG_DIR/"
  fi
  echo "rolled back to tag $TAG"
}

do_list() {
  mkdir -p "$BACKUP_ROOT"
  # Portable: GNU find -printf is not available on macOS/BSD.
  find "$BACKUP_ROOT" -mindepth 1 -maxdepth 1 -type d 2>/dev/null | sort || true
}

do_verify() {
  local health ready
  health="$(curl -sS -m "$TIMEOUT" -o /tmp/fabric-verify-health.$$ -w '%{http_code}' "$BASE_URL/health" || true)"
  ready="$(curl -sS -m "$TIMEOUT" -o /tmp/fabric-verify-ready.$$ -w '%{http_code}' "$BASE_URL/readyz" || true)"
  echo "health_http=$health"
  echo "readyz_http=$ready"
  if [[ "$health" != "200" ]]; then
    echo "verify failed: /health" >&2
    rm -f /tmp/fabric-verify-health.$$ /tmp/fabric-verify-ready.$$
    return 1
  fi
  rm -f /tmp/fabric-verify-health.$$ /tmp/fabric-verify-ready.$$
  echo "verify ok"
}

case "$cmd" in
  snapshot) do_snapshot ;;
  rollback) do_rollback ;;
  list) do_list ;;
  verify) do_verify ;;
  ""|-h|--help) usage ;;
  *) echo "unknown command: $cmd" >&2; usage; exit 2 ;;
esac
