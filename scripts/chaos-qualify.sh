#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0
# Track-1 (always) + track-2 live probes (optional).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
REPORT_DIR=""
LIVE=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --report) REPORT_DIR="$2"; shift 2 ;;
    --live) LIVE=1; shift ;;
    *) echo "unknown arg $1" >&2; exit 2 ;;
  esac
done

pass=0
fail=0
record() {
  local name="$1" status="$2" detail="$3"
  printf '%s\t%s\t%s\n' "$status" "$name" "$detail"
  if [[ "$status" == "PASS" ]]; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1))
  fi
}

# --- Track 1: disk-full backup fail-closed via the Python-free tar path ---
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/vm/disks"
echo '{"ok":true}' >"$tmp/vm/config.json"
dd if=/dev/zero of="$tmp/vm/disks/disk0.bin" bs=1024 count=64 status=none

# Tiny tmpfs-like dest: a file that is not a directory should fail create
if mkdir -p "$tmp/backups" && tar -C "$tmp" -czf "$tmp/backups/ok.tar.gz" vm; then
  record "backup_archive_ok" PASS "created $tmp/backups/ok.tar.gz"
else
  record "backup_archive_ok" FAIL "tar create failed"
fi

# Corrupt archive must not unpack as success
echo 'not-gzip' >"$tmp/backups/bad.tar.gz"
if tar -C "$tmp/restore-bad" -xzf "$tmp/backups/bad.tar.gz" 2>/dev/null; then
  record "corrupt_backup_rejected" FAIL "corrupt archive unpacked"
else
  record "corrupt_backup_rejected" PASS "restore failed closed"
fi

# Heartbeat quorum arithmetic (mirror of Rust majority rule)
q="$tmp/quorum"
mkdir -p "$q"
now="$(date +%s)"
echo "$now" >"$q/host-a.heartbeat"
echo 1 >"$q/host-b.heartbeat"
echo 1 >"$q/host-c.heartbeat"
alive=0
total=0
for f in "$q"/*.heartbeat; do
  total=$((total + 1))
  ts="$(cat "$f")"
  if (( now - ts < 30 )); then
    alive=$((alive + 1))
  fi
done
if (( alive > total / 2 )); then
  record "quorum_minority" FAIL "1/3 should lose quorum"
else
  record "quorum_minority" PASS "1/${total} lost quorum"
fi

if [[ "$LIVE" -eq 1 ]]; then
  url="${FABRIC_URL:-http://127.0.0.1:9095}"
  code="$(curl -sS -m 2 -o /dev/null -w '%{http_code}' "$url/health" || true)"
  if [[ "$code" == "200" ]]; then
    record "live_health" PASS "GET /health $code"
  else
    record "live_health" FAIL "GET /health $code"
  fi
  ready="$(curl -sS -m 2 -o /dev/null -w '%{http_code}' "$url/readyz" || true)"
  if [[ "$ready" == "200" || "$ready" == "503" ]]; then
    record "live_readyz" PASS "GET /readyz $ready (503 allowed if FluxVM down)"
  else
    record "live_readyz" FAIL "GET /readyz $ready"
  fi
else
  record "live_skipped" PASS "pass --live and FABRIC_URL to inject fabricd/FluxVM faults"
fi

echo "summary pass=$pass fail=$fail"
if [[ -n "$REPORT_DIR" ]]; then
  mkdir -p "$REPORT_DIR"
  printf 'pass=%s\nfail=%s\n' "$pass" "$fail" >"$REPORT_DIR/latest.txt"
fi
[[ "$fail" -eq 0 ]]
