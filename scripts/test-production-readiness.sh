#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: Apache-2.0
#
# Production readiness gate for Zyvor Fabric (control plane).
# Read-only by default — safe to run before admitting traffic.
#
# Required:
#   FABRIC_TOKEN or ZYVOR_FABRIC_TOKEN   (no lab Admin@321 fallback)
#
# Env:
#   FABRIC_URL=https://127.0.0.1:9095
#   FLUXVM_URL=http://127.0.0.1:7788
#   SECTIONS=all|doctor,devops,dataplane_ro,wireguard,edge_mutate
#   RUN_MUTATING=1     enable wireguard + edge write e2e (off by default)
#   REQUIRE_DOCTOR=1   FAIL if fabric-doctor binary missing
#
# Exit: 0 all required PASS; 1 FAIL; 2 misconfig
#
#   FABRIC_URL=https://fabric:9095 FABRIC_TOKEN=… ./scripts/test-production-readiness.sh
# Pre-rename names still work: KEEP_<X> is used when MERION_<X> is unset.
[ -n "${MERION_LAB+x}" ] || [ -z "${KEEP_LAB:-}" ] || export MERION_LAB="$KEEP_LAB"

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
exec </dev/null

FABRIC_URL="${FABRIC_URL:-https://127.0.0.1:9095}"
FLUXVM_URL="${FLUXVM_URL:-http://127.0.0.1:7788}"
export FABRIC_URL FLUXVM_URL
export ZYVOR_ALLOW_OFFLINE=0
export ZYVOR_CHECK_FLUXVM="${ZYVOR_CHECK_FLUXVM:-1}"

TOKEN="${FABRIC_TOKEN:-${ZYVOR_FABRIC_TOKEN:-}}"
SECTIONS_RAW="${SECTIONS:-all}"
RUN_MUTATING="${RUN_MUTATING:-0}"
BODY="$(mktemp)"
trap 'rm -f "$BODY"' EXIT

PASS_N=0
FAIL_N=0
SKIP_N=0
declare -a SUMMARY=()

pass() { PASS_N=$((PASS_N + 1)); SUMMARY+=("PASS|$1|${2:-}"); echo "  [PASS] $1${2:+ — $2}"; }
fail() { FAIL_N=$((FAIL_N + 1)); SUMMARY+=("FAIL|$1|${2:-}"); echo "  [FAIL] $1${2:+ — $2}" >&2; }
skip() { SKIP_N=$((SKIP_N + 1)); SUMMARY+=("SKIP|$1|${2:-}"); echo "  [SKIP] $1${2:+ — $2}"; }

truthy() {
  case "${1:-}" in 1|true|yes|TRUE|YES) return 0 ;; *) return 1 ;; esac
}

want_section() {
  local name="$1"
  if [[ "$SECTIONS_RAW" == "all" ]]; then
    case "$name" in
      wireguard|edge_mutate)
        truthy "$RUN_MUTATING" && return 0
        return 1
        ;;
      *) return 0 ;;
    esac
  fi
  [[ ",${SECTIONS_RAW}," == *",$name,"* ]]
}

misconfig() {
  echo "misconfig: $*" >&2
  exit 2
}

[[ -n "$TOKEN" ]] || misconfig "set FABRIC_TOKEN or ZYVOR_FABRIC_TOKEN (prod gate forbids lab password fallbacks)"

AUTH_H="Authorization: Bearer $TOKEN"

api_code() {
  local method="$1" path="$2"
  curl -sk -m 10 -X "$method" -H "$AUTH_H" -H "Content-Type: application/json" \
    -o "$BODY" -w '%{http_code}' \
    "${FABRIC_URL}${path}" 2>/dev/null || echo 000
}

echo "########## Fabric production readiness ##########"
echo "  FABRIC_URL=$FABRIC_URL FLUXVM_URL=$FLUXVM_URL"
echo "  SECTIONS=$SECTIONS_RAW RUN_MUTATING=$RUN_MUTATING"
echo ""

# ── doctor ──────────────────────────────────────────────────────────
if want_section doctor; then
  echo "=== doctor ==="
  DOCTOR_BIN=""
  if command -v fabric-doctor >/dev/null 2>&1; then
    DOCTOR_BIN="$(command -v fabric-doctor)"
  elif [[ -x "$ROOT/tools/fabric-doctor/target/release/fabric-doctor" ]]; then
    DOCTOR_BIN="$ROOT/tools/fabric-doctor/target/release/fabric-doctor"
  fi
  if [[ -z "$DOCTOR_BIN" ]]; then
    if truthy "${REQUIRE_DOCTOR:-0}"; then
      fail "doctor" "fabric-doctor binary not found (REQUIRE_DOCTOR=1)"
    else
      skip "doctor" "fabric-doctor not installed; set REQUIRE_DOCTOR=1 to fail"
    fi
  else
    set +e
    "$DOCTOR_BIN" check --strict-services --fabric-url "$FABRIC_URL" \
      >/tmp/fabric-doctor.out 2>&1
    dc=$?
    set -e
    if [[ $dc -eq 0 ]]; then
      pass "doctor" "$DOCTOR_BIN"
    else
      fail "doctor" "exit $dc (see /tmp/fabric-doctor.out)"
      tail -20 /tmp/fabric-doctor.out >&2 || true
    fi
  fi
fi

# ── devops ──────────────────────────────────────────────────────────
if want_section devops; then
  echo "=== devops ==="
  chmod +x "$ROOT/scripts/devops-gate.sh" 2>/dev/null || true
  set +e
  "$ROOT/scripts/devops-gate.sh" >/tmp/fabric-devops.out 2>&1
  dc=$?
  set -e
  if [[ $dc -eq 0 ]]; then
    pass "devops" "health+readyz"
  else
    fail "devops" "devops-gate exit $dc"
    cat /tmp/fabric-devops.out >&2 || true
  fi
fi

# ── dataplane_ro ────────────────────────────────────────────────────
if want_section dataplane_ro; then
  echo "=== dataplane_ro ==="
  ro_ok=1
  code="$(api_code GET /readyz)"
  if [[ "$code" != "200" ]]; then
    fail "dataplane_ro" "GET /readyz HTTP $code"
    ro_ok=0
  fi

  for path in \
    /api/dataplane/services/status \
    /api/dataplane/remote-backends \
    /api/dataplane/remote-identities \
    /api/vpn-tunnels \
    /api/vpn-tunnels/status
  do
    code="$(api_code GET "$path")"
    if [[ "$code" != "200" ]]; then
      fail "dataplane_ro" "GET $path HTTP $code"
      ro_ok=0
    fi
  done

  if [[ "$ro_ok" -eq 1 ]]; then
    code="$(api_code GET /api/dataplane/services/status)"
    if [[ "$code" == "200" ]]; then
      BODY="$BODY" python3 - <<'PY'
import json, os
doc = json.load(open(os.environ["BODY"]))
print(
    "    status schema_version=%s program_generation=%s map_tier=%s"
    % (doc.get("schema_version"), doc.get("program_generation"), doc.get("map_tier"))
)
PY
    fi
    pass "dataplane_ro" "readyz + services/status + remotes + vpn list/status"
  fi
fi

# ── wireguard (mutating) ────────────────────────────────────────────
if want_section wireguard; then
  echo "=== wireguard (mutating) ==="
  chmod +x "$ROOT/scripts/test-wireguard-service-fabric.sh" 2>/dev/null || true
  set +e
  FABRIC_TOKEN="$TOKEN" MERION_LAB="${MERION_LAB:-0}" \
    "$ROOT/scripts/test-wireguard-service-fabric.sh" >/tmp/fabric-wg.out 2>&1
  dc=$?
  set -e
  if [[ $dc -eq 0 ]]; then
    pass "wireguard" "underlay e2e"
  else
    fail "wireguard" "exit $dc"
    tail -40 /tmp/fabric-wg.out >&2 || true
  fi
elif [[ "$SECTIONS_RAW" == "all" ]]; then
  skip "wireguard" "read-only default (RUN_MUTATING=0)"
fi

# ── edge_mutate ─────────────────────────────────────────────────────
if want_section edge_mutate; then
  echo "=== edge_mutate ==="
  chmod +x "$ROOT/scripts/test-edge-dataplane-e2e.sh" 2>/dev/null || true
  set +e
  FABRIC_TOKEN="$TOKEN" "$ROOT/scripts/test-edge-dataplane-e2e.sh" >/tmp/fabric-edge.out 2>&1
  dc=$?
  set -e
  if [[ $dc -eq 0 ]]; then
    pass "edge_mutate" "edge dataplane e2e"
  else
    fail "edge_mutate" "exit $dc"
    tail -40 /tmp/fabric-edge.out >&2 || true
  fi
elif [[ "$SECTIONS_RAW" == "all" ]]; then
  skip "edge_mutate" "read-only default (RUN_MUTATING=0)"
fi

echo ""
echo "########## Summary ##########"
for row in "${SUMMARY[@]}"; do
  IFS='|' read -r st name detail <<<"$row"
  printf '  %-4s  %-14s  %s\n' "$st" "$name" "$detail"
done
echo "pass=$PASS_N fail=$FAIL_N skip=$SKIP_N"

if [[ "$FAIL_N" -gt 0 ]]; then
  echo "########## Fabric production readiness: FAIL ##########" >&2
  exit 1
fi
echo "########## Fabric production readiness: PASS ##########"
exit 0
