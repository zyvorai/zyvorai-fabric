#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SCRIPT="$ROOT/scripts/upgrade-rollback.sh"
WORKDIR="$(mktemp -d)"
trap 'rm -rf "$WORKDIR"' EXIT

STATE="$WORKDIR/var/lib/zyvor-fabricd"
CFG="$WORKDIR/etc/zyvor-fabricd"
SNAPS="$WORKDIR/snaps"

mkdir -p "$STATE/state" "$CFG"
echo 'vms = []' >"$STATE/state/store.json"
echo 'listen = "0.0.0.0:9095"' >"$CFG/zyvor-fabricd.toml"
echo 'super-secret' >"$STATE/.jwt_secret"
chmod 0600 "$STATE/.jwt_secret"

"$SCRIPT" snapshot --tag before-nplus1 \
  --state-dir "$STATE" --config-dir "$CFG" --backup-root "$SNAPS"

# Mutate as if N+1 wrote new state
echo 'broken' >"$STATE/state/store.json"
echo 'listen = "0.0.0.0:1"' >"$CFG/zyvor-fabricd.toml"

"$SCRIPT" rollback --tag before-nplus1 \
  --state-dir "$STATE" --config-dir "$CFG" --backup-root "$SNAPS"

grep -q 'vms = \[\]' "$STATE/state/store.json"
grep -q '9095' "$CFG/zyvor-fabricd.toml"
[[ "$(cat "$STATE/.jwt_secret")" == "super-secret" ]]

"$SCRIPT" list --backup-root "$SNAPS" | grep -q before-nplus1

echo "test-upgrade-rollback: ok"
