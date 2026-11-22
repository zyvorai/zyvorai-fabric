#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0
#
# Proven-infra P2 suites (#14–#17): harness, upgrade-rollback, chaos, optional cargo.
#
#   ./scripts/test-proven-infra.sh
#   RUN_CARGO=1 ./scripts/test-proven-infra.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

PASS=0
FAIL=0
ok() { PASS=$((PASS + 1)); echo "  [PASS] $1"; }
bad() { FAIL=$((FAIL + 1)); echo "  [FAIL] $1" >&2; }
section() { echo; echo "======== $1 ========"; }

section "harness"
if python3 -m unittest benchmarks.test_harness -v 2>&1 | tee /tmp/fabric-harness.out | tail -25; then
  grep -q 'OK' /tmp/fabric-harness.out && ok "benchmarks.test_harness" || bad "harness"
else
  bad "harness exit"
fi

section "upgrade-rollback"
chmod +x scripts/test-upgrade-rollback.sh scripts/upgrade-rollback.sh
if bash scripts/test-upgrade-rollback.sh 2>&1 | tee /tmp/fabric-upgrade.out; then
  grep -q 'test-upgrade-rollback: ok' /tmp/fabric-upgrade.out && ok "upgrade-rollback" || bad "upgrade-rollback"
else
  bad "upgrade-rollback exit"
fi

section "chaos-qualify"
chmod +x scripts/chaos-qualify.sh
if bash scripts/chaos-qualify.sh 2>&1 | tee /tmp/fabric-chaos.out | tail -15; then
  grep -q 'fail=0' /tmp/fabric-chaos.out && ok "chaos-qualify" || bad "chaos-qualify"
else
  bad "chaos-qualify exit"
fi

if [[ "${RUN_CARGO:-0}" == "1" ]]; then
  section "cargo backup + fault-tolerance"
  if (cd backend && cargo test -p backup -p fault-tolerance 2>&1 | tee /tmp/fabric-cargo-bf.out | tail -30); then
    grep -q 'test result: ok' /tmp/fabric-cargo-bf.out && ok "cargo backup/fault-tolerance" || bad "cargo"
  else
    bad "cargo exit"
  fi
else
  echo "  (set RUN_CARGO=1 to run cargo test -p backup -p fault-tolerance)"
fi

echo
echo "proven-infra PASS=$PASS FAIL=$FAIL"
exit $((FAIL > 0 ? 1 : 0))
