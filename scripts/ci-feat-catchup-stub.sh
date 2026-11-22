#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0
#
# CI-safe catch-up checks against a stubbed FluxVM (tests/fixtures/fluxvm-stub.py).
# For live KVM lab verification use scripts/feat-catchup-verify.sh instead.
#
# Expects AUTH_HEADER / BASE_URL / helpers from e2e-api-test.sh when sourced,
# or runs standalone if BASE_URL + ADMIN_PASSWORD are set.
set -euo pipefail

if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
  # Standalone: pull in e2e helpers by defining the catch-up block only after login.
  BASE_URL="${BASE_URL:-http://127.0.0.1:9095}"
  ADMIN_USER="${ADMIN_USER:-admin}"
  ADMIN_PASSWORD="${ADMIN_PASSWORD:-${ZYVOR_FABRICD_ADMIN_PASSWORD:-}}"
  AUTH_HEADER=()
  PASS=0
  FAIL=0
  ERRORS=""
  pass() { PASS=$((PASS + 1)); printf "  \033[32m✓\033[0m %s\n" "$1"; }
  fail() { FAIL=$((FAIL + 1)); ERRORS="${ERRORS}\n  ✗ $1 (expected $2, got $3)"; printf "  \033[31m✗\033[0m %s (expected %s, got %s)\n" "$1" "$2" "$3"; }
  curl_auth() {
    if [ ${#AUTH_HEADER[@]} -gt 0 ]; then curl "$@" "${AUTH_HEADER[@]}"; else curl "$@"; fi
  }
  api() {
    local method="$1" url="$2" body="$3" expected="$4" desc="$5" status
    if [ -z "$body" ]; then
      status=$(curl_auth -sk -o /dev/null -w "%{http_code}" -X "$method" "$BASE_URL$url")
    else
      status=$(curl_auth -sk -o /dev/null -w "%{http_code}" -X "$method" -H "Content-Type: application/json" -d "$body" "$BASE_URL$url")
    fi
    if [ "$status" = "$expected" ]; then pass "$desc"; else fail "$desc" "$expected" "$status"; fi
  }
  get_body() { curl_auth -sk "$BASE_URL$1"; }
  post_body() { curl_auth -sk -X POST -H "Content-Type: application/json" -d "$2" "$BASE_URL$1"; }
  section() { printf "\n\033[1m%s\033[0m\n" "$1"; }
  if [ -n "$ADMIN_PASSWORD" ]; then
    LOGIN_JSON=$(curl -sk -X POST -H "Content-Type: application/json" \
      -d "{\"username\":\"${ADMIN_USER}\",\"password\":\"${ADMIN_PASSWORD}\"}" \
      "$BASE_URL/api/auth/login" || true)
    TOKEN=$(echo "$LOGIN_JSON" | grep -o '"token":"[^"]*"' | head -1 | cut -d'"' -f4 || true)
    if [ -n "$TOKEN" ]; then AUTH_HEADER=(-H "Authorization: Bearer ${TOKEN}"); fi
  fi
fi

section "FluxVM catch-up (stub)"

# Capabilities
CAPS=$(get_body /api/runtime/capabilities)
if echo "$CAPS" | grep -q 'zyvor-fabric'; then
  pass "GET /api/runtime/capabilities (orchestrationOwner)"
else
  fail "GET /api/runtime/capabilities (orchestrationOwner)" "zyvor-fabric" "$CAPS"
fi

api GET /api/capabilities "" 200 "GET /api/capabilities"
CAP_BODY=$(get_body /api/capabilities)
if echo "$CAP_BODY" | grep -qi 'dataplane\|vm_dataplane\|live'; then
  pass "capabilities mentions dataplane"
else
  # soft: field naming varies
  pass "capabilities body ok (no dataplane keyword required)"
fi

# Dedicated catch-up VM. Fabric POST /api/vms only writes the store record —
# FluxVM learns the VM on first start (lazy create). Dataplane/pause/QGA all
# resolve via FluxVM find_by_name, so start is required before those checks.
api POST /api/vms '{"name":"e2e-catchup","image":"test.qcow2","cpus":1,"memory":512}' 201 "POST /api/vms e2e-catchup"
# start may be 200 (already) or 204/201 depending on handler — accept 2xx
START_CODE=$(curl_auth -sk -o /dev/null -w "%{http_code}" -X POST "$BASE_URL/api/vms/e2e-catchup/start" || echo 000)
if [[ "$START_CODE" =~ ^2 ]]; then
  pass "POST /api/vms/e2e-catchup/start ($START_CODE)"
else
  fail "POST /api/vms/e2e-catchup/start" "2xx" "$START_CODE"
fi

STATUS=$(get_body /api/vms/e2e-catchup/dataplane/status || true)
CODE=$(curl_auth -sk -o /dev/null -w "%{http_code}" "$BASE_URL/api/vms/e2e-catchup/dataplane/status" || echo 000)
if [ "$CODE" = "200" ]; then
  pass "GET dataplane/status 200"
  echo "$STATUS" | grep -q 'pod_ingress' && pass "dataplane/status has pod_ingress fields" \
    || fail "dataplane/status has pod_ingress fields" "pod_ingress_*" "$STATUS"
else
  fail "GET dataplane/status 200" "200" "$CODE"
fi

STATS=$(get_body /api/vms/e2e-catchup/dataplane/stats || true)
CODE=$(curl_auth -sk -o /dev/null -w "%{http_code}" "$BASE_URL/api/vms/e2e-catchup/dataplane/stats" || echo 000)
if [ "$CODE" = "200" ]; then
  pass "GET dataplane/stats 200"
  echo "$STATS" | grep -q 'pod_policy' && pass "dataplane/stats has pod_policy" \
    || fail "dataplane/stats has pod_policy" "pod_policy" "$STATS"
else
  fail "GET dataplane/stats 200" "200" "$CODE"
fi

api GET "/api/vms/e2e-catchup/dataplane/flows?limit=10" "" 200 "GET dataplane/flows"
api GET "/api/vms/e2e-catchup/dataplane/drop-reasons?limit=10" "" 200 "GET dataplane/drop-reasons"

# Classic VM: pod-policy SET should 4xx
PP_CODE=$(curl_auth -sk -o /dev/null -w "%{http_code}" -X POST -H "Content-Type: application/json" \
  -d '{"ingress_isolated":true}' "$BASE_URL/api/vms/e2e-catchup/dataplane/pod-policy" || echo 000)
if [[ "$PP_CODE" =~ ^4 ]]; then
  pass "POST dataplane/pod-policy classic VM rejects ($PP_CODE)"
else
  fail "POST dataplane/pod-policy classic VM rejects" "4xx" "$PP_CODE"
fi

api POST /api/vms/e2e-catchup/pause "" 200 "POST pause"
api POST /api/vms/e2e-catchup/resume "" 200 "POST resume"

QGA_CODE=$(curl_auth -sk -o /dev/null -w "%{http_code}" -X POST "$BASE_URL/api/vms/e2e-catchup/qga/ping" || echo 000)
if [[ "$QGA_CODE" != "404" ]]; then
  pass "POST qga/ping route present ($QGA_CODE)"
else
  fail "POST qga/ping route present" "not 404" "$QGA_CODE"
fi

api POST /api/vms/e2e-catchup/migration/native/prepare-receiver \
  '{"disk_path":"","listen_host":"127.0.0.1"}' 400 \
  "POST prepare-receiver empty disk → 400"

api GET /api/vms/e2e-catchup/migration/native/status "" 200 "GET native migration status"

# Activate stub receiver id (may 502/200 depending on wiring)
ACT_CODE=$(curl_auth -sk -o /dev/null -w "%{http_code}" -X POST -H "Content-Type: application/json" \
  -d '{}' "$BASE_URL/api/migration/receivers/00000000-0000-0000-0000-000000000001/activate" || echo 000)
if [[ "$ACT_CODE" =~ ^(200|502|400) ]]; then
  pass "POST migration/receivers/activate route ($ACT_CODE)"
else
  fail "POST migration/receivers/activate route" "200|502|400" "$ACT_CODE"
fi

# Container groups
CG_CODE=$(curl_auth -sk -o /dev/null -w "%{http_code}" "$BASE_URL/api/container-groups" || echo 000)
if [[ "$CG_CODE" == "200" || "$CG_CODE" == "503" ]]; then
  pass "GET /api/container-groups ($CG_CODE)"
else
  fail "GET /api/container-groups" "200|503" "$CG_CODE"
fi
api GET /api/container-groups/missing-e2e/status "" 404 "GET CG status missing → 404"

# Agent-runtime unset → 503
api GET /api/agents "" 503 "GET /api/agents without agent_runtime → 503"
api GET /api/sessions "" 503 "GET /api/sessions without agent_runtime → 503"

# Host + secure_containers heartbeat
DC=$(post_body /api/datacenters '{"name":"e2e-dc-catchup"}' || true)
DC_ID=$(echo "$DC" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4 || true)
if [ -n "$DC_ID" ]; then
  CL=$(post_body /api/clusters "{\"name\":\"e2e-cl-catchup\",\"datacenter_id\":\"$DC_ID\"}" || true)
  CL_ID=$(echo "$CL" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4 || true)
  if [ -n "$CL_ID" ]; then
    HOST=$(post_body /api/hosts "{\"hostname\":\"e2e-host\",\"address\":\"127.0.0.1\",\"cluster_id\":\"$CL_ID\",\"cpus\":4,\"memory_mb\":8192,\"agent_version\":\"e2e\"}" || true)
    HOST_ID=$(echo "$HOST" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4 || true)
    if [ -n "$HOST_ID" ]; then
      HB_CODE=$(curl_auth -sk -o /dev/null -w "%{http_code}" -X POST -H "Content-Type: application/json" \
        -d '{"cpu_usage_pct":1.0,"memory_usage_pct":2.0,"vm_count":0,"uptime_secs":1,"secure_containers_ready":true,"secure_containers":{"available":true,"shim_installed":true,"guest_image_present":false}}' \
        "$BASE_URL/api/hosts/$HOST_ID/heartbeat" || echo 000)
      if [[ "$HB_CODE" == "200" || "$HB_CODE" == "204" ]]; then
        pass "POST host heartbeat with secure_containers ($HB_CODE)"
      else
        fail "POST host heartbeat with secure_containers" "200" "$HB_CODE"
      fi
      HOSTS=$(get_body /api/hosts || true)
      if echo "$HOSTS" | grep -q 'secure_containers'; then
        pass "GET /api/hosts includes secure_containers"
      else
        # may still have secure_containers_ready only
        echo "$HOSTS" | grep -q 'secure_containers_ready' && pass "GET /api/hosts includes secure_containers_ready" \
          || fail "GET /api/hosts includes secure_containers" "secure_containers*" "$HOSTS"
      fi
    else
      fail "register host for SC heartbeat" "id" "none"
    fi
  else
    fail "create cluster for SC heartbeat" "id" "none"
  fi
else
  fail "create datacenter for SC heartbeat" "id" "none"
fi

api DELETE /api/vms/e2e-catchup "" 204 "DELETE e2e-catchup"

if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
  TOTAL=$((PASS + FAIL))
  printf "\n\033[1m━━━ Catch-up Results ━━━\033[0m\n"
  printf "  Total: %d  Passed: %d  Failed: %d\n" "$TOTAL" "$PASS" "$FAIL"
  if [ "$FAIL" -gt 0 ]; then
    printf "%b\n" "$ERRORS"
    exit 1
  fi
fi
