#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0
# End-to-end API test suite for zyvor-fabricd
# Runs against a live server at BASE_URL (default: http://localhost:8080)
#
set -euo pipefail

BASE_URL="${BASE_URL:-http://localhost:8080}"
ADMIN_USER="${ADMIN_USER:-admin}"
ADMIN_PASSWORD="${ADMIN_PASSWORD:-${ZYVOR_FABRICD_ADMIN_PASSWORD:-}}"
AUTH_HEADER=()
PASS=0
FAIL=0
ERRORS=""

# ─── Helpers ──────────────────────────────────────────────────────────────────

pass() {
  PASS=$((PASS + 1))
  printf "  \033[32m✓\033[0m %s\n" "$1"
}

fail() {
  FAIL=$((FAIL + 1))
  ERRORS="${ERRORS}\n  ✗ $1 (expected $2, got $3)"
  printf "  \033[31m✗\033[0m %s (expected %s, got %s)\n" "$1" "$2" "$3"
}

# Expand optional auth headers without tripping `set -u` on empty arrays.
curl_auth() {
  if [ ${#AUTH_HEADER[@]} -gt 0 ]; then
    curl "$@" "${AUTH_HEADER[@]}"
  else
    curl "$@"
  fi
}

# GET  url expected_status description
get() {
  local status
  status=$(curl_auth -sk -o /dev/null -w "%{http_code}" "$BASE_URL$1")
  if [ "$status" = "$2" ]; then pass "$3"; else fail "$3" "$2" "$status"; fi
}

# POST url body expected_status description
post() {
  local status
  if [ -z "$2" ]; then
    status=$(curl_auth -sk -o /dev/null -w "%{http_code}" -X POST "$BASE_URL$1")
  else
    status=$(curl_auth -sk -o /dev/null -w "%{http_code}" -X POST -H "Content-Type: application/json" -d "$2" "$BASE_URL$1")
  fi
  if [ "$status" = "$2" ] || [ "$status" = "$3" ]; then pass "$4"; else fail "$4" "$3" "$status"; fi
}

# POST that returns body for further use
post_body() {
  curl_auth -sk -X POST -H "Content-Type: application/json" -d "$2" "$BASE_URL$1"
}

# GET that returns body
get_body() {
  curl_auth -sk "$BASE_URL$1"
}

# Extract "id" field from JSON string
extract_id() {
  echo "$1" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4 || echo ""
}

# Generic method tester: method url [body] expected_status description
api() {
  local method="$1" url="$2" body="$3" expected="$4" desc="$5"
  local status
  if [ -z "$body" ]; then
    status=$(curl_auth -sk -o /dev/null -w "%{http_code}" -X "$method" "$BASE_URL$url")
  else
    status=$(curl_auth -sk -o /dev/null -w "%{http_code}" -X "$method" -H "Content-Type: application/json" -d "$body" "$BASE_URL$url")
  fi
  if [ "$status" = "$expected" ]; then pass "$desc"; else fail "$desc" "$expected" "$status"; fi
}

# Optional login so write endpoints succeed when auth is enabled
if [ -n "$ADMIN_PASSWORD" ]; then
  LOGIN_JSON=$(curl -sk -X POST -H "Content-Type: application/json" \
    -d "{\"username\":\"${ADMIN_USER}\",\"password\":\"${ADMIN_PASSWORD}\"}" \
    "$BASE_URL/api/auth/login" || true)
  TOKEN=$(echo "$LOGIN_JSON" | grep -o '"token":"[^"]*"' | head -1 | cut -d'"' -f4 || true)
  if [ -n "$TOKEN" ]; then
    AUTH_HEADER=(-H "Authorization: Bearer ${TOKEN}")
    echo "Authenticated as ${ADMIN_USER}"
  else
    echo "WARNING: login failed; continuing unauthenticated (writes may 401/403)"
    echo "  response: ${LOGIN_JSON:-<empty>}"
  fi
fi

section() {
  printf "\n\033[1m%s\033[0m\n" "$1"
}

# ─── Health ───────────────────────────────────────────────────────────────────

section "Health & Metrics"
api GET /health "" 200 "GET /health"
api GET /metrics "" 200 "GET /metrics"

# ─── System ───────────────────────────────────────────────────────────────────

section "System (CPU/NUMA/Memory)"
api GET /api/system/cpu/topology "" 200 "GET /api/system/cpu/topology"
api GET /api/system/numa/topology "" 200 "GET /api/system/numa/topology"
api GET /api/system/memory "" 200 "GET /api/system/memory"
api GET "/api/system/memory/hugepages?size=Size2MB" "" 200 "GET /api/system/memory/hugepages"
api GET /api/system/firmware/capabilities "" 200 "GET /api/system/firmware/capabilities"
api GET /api/system/optimization/recommendations "" 200 "GET /api/system/optimization/recommendations"

# ─── VMs ──────────────────────────────────────────────────────────────────────

section "VMs"
api GET /api/vms "" 200 "GET /api/vms (list)"
api GET /api/vms/nonexistent "" 404 "GET /api/vms/nonexistent (not found)"

# Create a VM
api POST /api/vms '{"name":"e2e-vm","image":"test.qcow2","cpus":1,"memory":512}' 201 "POST /api/vms (create)"
api GET /api/vms/e2e-vm "" 200 "GET /api/vms/e2e-vm (get created)"
api GET /api/vms/e2e-vm/metrics "" 200 "GET /api/vms/:name/metrics"

# Clone VM
api POST /api/vms/e2e-vm/clone '{"target_name":"e2e-vm-clone","linked_clone":false}' 201 "POST /api/vms/:name/clone"
api GET /api/vms/e2e-vm-clone "" 200 "GET cloned VM"

# Cleanup VMs
api DELETE /api/vms/e2e-vm-clone "" 204 "DELETE /api/vms/e2e-vm-clone"
api DELETE /api/vms/e2e-vm "" 204 "DELETE /api/vms/e2e-vm"
api GET /api/vms/e2e-vm "" 404 "GET /api/vms/e2e-vm (verify deleted)"

# ─── Templates ────────────────────────────────────────────────────────────────

section "Templates"
api GET /api/templates "" 200 "GET /api/templates (list)"

TMPL=$(post_body /api/templates '{"name":"e2e-tmpl","cpus":2,"memory":2048,"disk":20,"image":"test.qcow2","tags":["test"]}')
TMPL_ID=$(extract_id "$TMPL")

if [ -n "$TMPL_ID" ]; then
  pass "POST /api/templates (create, id=$TMPL_ID)"
  api GET "/api/templates/$TMPL_ID" "" 200 "GET /api/templates/:id"
  api PUT "/api/templates/$TMPL_ID" '{"name":"e2e-tmpl-v2","cpus":4}' 200 "PUT /api/templates/:id (update)"
  api POST "/api/templates/$TMPL_ID/deploy" '{"vm_name":"e2e-from-tmpl"}' 201 "POST /api/templates/:id/deploy"
  api DELETE /api/vms/e2e-from-tmpl "" 204 "DELETE deployed VM"
  api DELETE "/api/templates/$TMPL_ID" "" 204 "DELETE /api/templates/:id"
  api GET "/api/templates/$TMPL_ID" "" 404 "GET /api/templates/:id (verify deleted)"
else
  fail "POST /api/templates (create)" "201" "no id"
fi

# ─── Storage Pools ────────────────────────────────────────────────────────────

section "Storage Pools"
api GET /api/storage/pools "" 200 "GET /api/storage/pools (list)"
api GET /api/storage/pools/nonexistent "" 404 "GET /api/storage/pools/nonexistent (not found)"

api POST /api/storage/pools/local '{"name":"e2e-pool","path":"/var/lib/zyvor-fabricd/e2e-pool","auto_start":true}' 200 "POST /api/storage/pools/local (create)"
api GET /api/storage/pools/e2e-pool "" 200 "GET /api/storage/pools/:name"
api DELETE /api/storage/pools/e2e-pool "" 204 "DELETE /api/storage/pools/:name"

# ─── Quotas ───────────────────────────────────────────────────────────────────

section "Quotas"
api GET /api/quotas "" 200 "GET /api/quotas (list)"
api GET /api/quotas/usage "" 200 "GET /api/quotas/usage (all usage)"

QUOTA=$(post_body /api/quotas '{"name":"e2e-quota","max_cpus":16,"max_memory":32768,"max_disk":500,"max_vms":10}')
QUOTA_ID=$(extract_id "$QUOTA")

if [ -n "$QUOTA_ID" ]; then
  pass "POST /api/quotas (create, id=$QUOTA_ID)"
  api GET "/api/quotas/$QUOTA_ID" "" 200 "GET /api/quotas/:id"
  api PUT "/api/quotas/$QUOTA_ID" '{"max_cpus":32}' 200 "PUT /api/quotas/:id (update)"
  api GET "/api/quotas/$QUOTA_ID/usage" "" 200 "GET /api/quotas/:id/usage"
  api POST "/api/quotas/$QUOTA_ID/enable" "" 200 "POST /api/quotas/:id/enable"
  api POST "/api/quotas/$QUOTA_ID/disable" "" 200 "POST /api/quotas/:id/disable"
  api DELETE "/api/quotas/$QUOTA_ID" "" 204 "DELETE /api/quotas/:id"
else
  fail "POST /api/quotas (create)" "201" "no id"
fi

# ─── Schedules ────────────────────────────────────────────────────────────────

section "Schedules"
api GET /api/schedules "" 200 "GET /api/schedules (list)"
api GET /api/schedules/history "" 200 "GET /api/schedules/history"

SCHED=$(post_body /api/schedules '{"name":"e2e-sched","vm_name":"test","action":"stop","schedule_type":"daily","time":"03:00"}')
SCHED_ID=$(extract_id "$SCHED")

if [ -n "$SCHED_ID" ]; then
  pass "POST /api/schedules (create, id=$SCHED_ID)"
  api GET "/api/schedules/$SCHED_ID" "" 200 "GET /api/schedules/:id"
  api PUT "/api/schedules/$SCHED_ID" '{"time":"04:00"}' 200 "PUT /api/schedules/:id (update)"
  api POST "/api/schedules/$SCHED_ID/enable" "" 200 "POST /api/schedules/:id/enable"
  api POST "/api/schedules/$SCHED_ID/disable" "" 200 "POST /api/schedules/:id/disable"
  api GET "/api/schedules/$SCHED_ID/history" "" 200 "GET /api/schedules/:id/history"
  api DELETE "/api/schedules/$SCHED_ID" "" 204 "DELETE /api/schedules/:id"
else
  fail "POST /api/schedules (create)" "201" "no id"
fi

# ─── Profiles ─────────────────────────────────────────────────────────────────

section "Profiles"
api GET /api/profiles "" 200 "GET /api/profiles (list)"
api POST /api/profiles '{"name":"e2e-profile","description":"Test","cpus":2,"memory":4096,"disk":40,"category":"general"}' 201 "POST /api/profiles (create)"
api GET /api/profiles/e2e-profile "" 200 "GET /api/profiles/:name"
api DELETE /api/profiles/e2e-profile "" 204 "DELETE /api/profiles/:name"

# ─── Migrations ───────────────────────────────────────────────────────────────

section "Migrations"
api GET /api/migrations "" 200 "GET /api/migrations (list)"
api GET /api/migrations/nonexistent "" 404 "GET /api/migrations/:id (not found)"

# ─── Images ───────────────────────────────────────────────────────────────────

section "Images"
api GET /api/images "" 200 "GET /api/images (list)"
# /api/images/builds never existed as a route (mkosi builds were removed --
# see the doc comment on images.rs's job-state enum); /images/downloads is
# the current equivalent job-listing endpoint (cloud-image downloads).
api GET /api/images/downloads "" 200 "GET /api/images/downloads (list)"

# ─── Plugins ──────────────────────────────────────────────────────────────────

section "Plugins"
api GET /api/plugins "" 200 "GET /api/plugins (list)"

# ─── Analytics ────────────────────────────────────────────────────────────────

section "Analytics"
api GET "/api/analytics/system?range=1h" "" 200 "GET /api/analytics/system"
api GET /api/analytics/insights "" 200 "GET /api/analytics/insights"
api GET /api/analytics/utilization "" 200 "GET /api/analytics/utilization"
api GET "/api/analytics/top?resource=cpu&limit=5" "" 200 "GET /api/analytics/top"

# ─── Audit ────────────────────────────────────────────────────────────────────

section "Audit"
api GET /api/audit/logs "" 200 "GET /api/audit/logs (list)"
api GET /api/audit/stats "" 200 "GET /api/audit/stats"

# ─── Notifications ────────────────────────────────────────────────────────────

section "Notifications"
api GET /api/notifications/channels "" 200 "GET /api/notifications/channels (list)"
api GET /api/notifications/rules "" 200 "GET /api/notifications/rules (list)"
api GET "/api/notifications/history?limit=10" "" 200 "GET /api/notifications/history"

CHAN=$(post_body /api/notifications/channels '{"name":"e2e-chan","type":"webhook","config":{"url":"https://example.com/hook"}}')
CHAN_ID=$(extract_id "$CHAN")

if [ -n "$CHAN_ID" ]; then
  pass "POST /api/notifications/channels (create)"
  api PUT "/api/notifications/channels/$CHAN_ID" '{"name":"e2e-chan-v2"}' 200 "PUT /api/notifications/channels/:id"
  api DELETE "/api/notifications/channels/$CHAN_ID" "" 204 "DELETE /api/notifications/channels/:id"
else
  fail "POST /api/notifications/channels" "201" "no id"
fi

# Note: rule creation requires valid channel IDs, tested via channels lifecycle above

# ─── Backups ──────────────────────────────────────────────────────────────────

section "Backups"
api GET /api/backups "" 200 "GET /api/backups (list)"
api GET /api/backups/jobs "" 200 "GET /api/backups/jobs (list)"
api GET /api/backups/policies "" 200 "GET /api/backups/policies (list)"
api GET /api/backups/stats "" 200 "GET /api/backups/stats"

# ─── Datacenter / Clusters / Hosts ────────────────────────────────────────────

section "Datacenters & Clusters & Hosts"
api GET /api/datacenters "" 200 "GET /api/datacenters (list)"
api GET /api/clusters "" 200 "GET /api/clusters (list)"
api GET /api/hosts "" 200 "GET /api/hosts (list)"

DC=$(post_body /api/datacenters '{"name":"e2e-dc","description":"E2E test datacenter"}')
DC_ID=$(extract_id "$DC")

if [ -n "$DC_ID" ]; then
  pass "POST /api/datacenters (create)"
  api GET "/api/datacenters/$DC_ID" "" 200 "GET /api/datacenters/:id"
  api GET "/api/datacenters/$DC_ID/summary" "" 200 "GET /api/datacenters/:id/summary"
  api PUT "/api/datacenters/$DC_ID" '{"name":"e2e-dc-v2"}' 200 "PUT /api/datacenters/:id"

  # Cluster
  CL=$(post_body /api/clusters "{\"name\":\"e2e-cluster\",\"datacenter_id\":\"$DC_ID\",\"description\":\"test\",\"ha_enabled\":false,\"drs_enabled\":false,\"drs_mode\":\"manual\"}")
  CL_ID=$(extract_id "$CL")

  if [ -n "$CL_ID" ]; then
    pass "POST /api/clusters (create)"
    api GET "/api/clusters/$CL_ID" "" 200 "GET /api/clusters/:id"
    api GET "/api/clusters/$CL_ID/health" "" 200 "GET /api/clusters/:id/health"
    api PUT "/api/clusters/$CL_ID" '{"name":"e2e-cluster-v2"}' 200 "PUT /api/clusters/:id"
    api DELETE "/api/clusters/$CL_ID" "" 204 "DELETE /api/clusters/:id"
  else
    fail "POST /api/clusters" "201" "no id"
  fi

  # Host
  HOST=$(post_body /api/hosts '{"hostname":"e2e-host","address":"10.0.0.99","cluster_id":"","cpus":8,"memory_mb":16384,"agent_version":"0.1.0"}')
  HOST_ID=$(extract_id "$HOST")

  if [ -n "$HOST_ID" ]; then
    pass "POST /api/hosts (register)"
    api GET "/api/hosts/$HOST_ID" "" 200 "GET /api/hosts/:id"
    api POST "/api/hosts/$HOST_ID/heartbeat" '{"cpu_usage_pct":10.0,"memory_usage_pct":20.0,"vm_count":0,"uptime_secs":100}' 200 "POST /api/hosts/:id/heartbeat"
    api POST "/api/hosts/$HOST_ID/maintenance/enter" "" 200 "POST /api/hosts/:id/maintenance/enter"
    api POST "/api/hosts/$HOST_ID/maintenance/exit" "" 200 "POST /api/hosts/:id/maintenance/exit"
    api DELETE "/api/hosts/$HOST_ID" "" 204 "DELETE /api/hosts/:id"
  else
    fail "POST /api/hosts" "201" "no id"
  fi

  api DELETE "/api/datacenters/$DC_ID" "" 204 "DELETE /api/datacenters/:id"
else
  fail "POST /api/datacenters" "201" "no id"
fi

# ─── Resource Pools ───────────────────────────────────────────────────────────

section "Resource Pools"
api GET /api/resource-pools "" 200 "GET /api/resource-pools (list)"

RP=$(post_body /api/resource-pools '{"name":"e2e-rp","cluster_id":"none","cpu_shares":"normal","cpu_reservation_mhz":0,"cpu_limit_mhz":0,"memory_shares":"normal","memory_reservation_mb":0,"memory_limit_mb":0,"cpu_expandable_reservation":true,"memory_expandable_reservation":true}')
RP_ID=$(extract_id "$RP")

if [ -n "$RP_ID" ]; then
  pass "POST /api/resource-pools (create)"
  api GET "/api/resource-pools/$RP_ID" "" 200 "GET /api/resource-pools/:id"
  api GET "/api/resource-pools/$RP_ID/summary" "" 200 "GET /api/resource-pools/:id/summary"
  api PUT "/api/resource-pools/$RP_ID" '{"name":"e2e-rp-v2"}' 200 "PUT /api/resource-pools/:id"
  api POST "/api/resource-pools/$RP_ID/admission" '{"cpu":2,"memory_mb":1024}' 422 "POST /api/resource-pools/:id/admission (exceeds zero limits)"
  api DELETE "/api/resource-pools/$RP_ID" "" 204 "DELETE /api/resource-pools/:id"
else
  fail "POST /api/resource-pools" "201" "no id"
fi

# ─── DRS ──────────────────────────────────────────────────────────────────────

section "DRS"
api GET /api/drs/affinity-rules "" 200 "GET /api/drs/affinity-rules (list)"

# ─── Distributed Storage ─────────────────────────────────────────────────────

section "Distributed Storage"
api GET /api/distributed-storage/pools "" 200 "GET /api/distributed-storage/pools (list)"
api GET /api/distributed-storage/migrations "" 200 "GET /api/distributed-storage/migrations (list)"
api GET /api/distributed-storage/policies "" 200 "GET /api/distributed-storage/policies (list)"
api GET /api/distributed-storage/datastore-clusters "" 200 "GET /api/distributed-storage/datastore-clusters (list)"

# ─── Encryption ───────────────────────────────────────────────────────────────

section "Encryption"
api GET /api/encryption/providers "" 200 "GET /api/encryption/providers (list)"
api GET /api/encryption/policies "" 200 "GET /api/encryption/policies (list)"
api GET /api/encryption/vms "" 200 "GET /api/encryption/vms (list)"

# ─── Networking (systemd-networkd) ────────────────────────────────────────────

section "Networking (systemd-networkd)"
api GET /api/networkd/bridges "" 200 "GET /api/networkd/bridges (list)"
api GET /api/networkd/vlans "" 200 "GET /api/networkd/vlans (list)"
api GET /api/networkd/macvtaps "" 200 "GET /api/networkd/macvtaps (list)"
api GET /api/networkd/taps "" 200 "GET /api/networkd/taps (list)"
api GET /api/networkd/bonds "" 200 "GET /api/networkd/bonds (list)"
api GET /api/networkd/network-files "" 200 "GET /api/networkd/network-files (list)"
api GET /api/networkd/link-files "" 200 "GET /api/networkd/link-files (list)"
api GET /api/networkd/links "" 200 "GET /api/networkd/links (list)"
api GET /api/networkd/port-forwards "" 200 "GET /api/networkd/port-forwards (list)"
api GET /api/networkd/files "" 200 "GET /api/networkd/files (list)"

# Bridge lifecycle
BR=$(post_body /api/networkd/bridges '{"name":"br-e2e","stp":false,"addresses":["10.99.0.1/24"]}')
BR_ID=$(extract_id "$BR")

if [ -n "$BR_ID" ]; then
  pass "POST /api/networkd/bridges (create)"
  api GET "/api/networkd/bridges/$BR_ID" "" 200 "GET /api/networkd/bridges/:id"
  api PUT "/api/networkd/bridges/$BR_ID" '{"name":"br-e2e","stp":true}' 200 "PUT /api/networkd/bridges/:id"
  api DELETE "/api/networkd/bridges/$BR_ID" "" 204 "DELETE /api/networkd/bridges/:id"
else
  fail "POST /api/networkd/bridges" "201" "no id"
fi

# ─── Fault Tolerance ──────────────────────────────────────────────────────────

section "Fault Tolerance"
api GET /api/ft/vms "" 200 "GET /api/ft/vms (list)"
api GET /api/ft/events "" 200 "GET /api/ft/events (list)"

# ─── Replication ──────────────────────────────────────────────────────────────

section "Replication"
api GET /api/replication/sites "" 200 "GET /api/replication/sites (list)"
api GET /api/replication/configs "" 200 "GET /api/replication/configs (list)"
api GET /api/replication/rpo-violations "" 200 "GET /api/replication/rpo-violations"
api GET /api/replication/health "" 200 "GET /api/replication/health"

# ─── Site Recovery ────────────────────────────────────────────────────────────

section "Site Recovery"
api GET /api/site-recovery/plans "" 200 "GET /api/site-recovery/plans (list)"
api GET /api/site-recovery/executions "" 200 "GET /api/site-recovery/executions (list)"
api GET /api/site-recovery/dashboard "" 200 "GET /api/site-recovery/dashboard"

# ─── Content Library ──────────────────────────────────────────────────────────

section "Content Library"
api GET /api/content-library/libraries "" 200 "GET /api/content-library/libraries (list)"
api GET /api/content-library/customization-specs "" 200 "GET /api/content-library/customization-specs (list)"
api GET /api/content-library/host-profiles "" 200 "GET /api/content-library/host-profiles (list)"

# ─── Lifecycle Manager ────────────────────────────────────────────────────────

section "Lifecycle Manager"
api GET /api/lifecycle/baselines "" 200 "GET /api/lifecycle/baselines (list)"
api GET /api/lifecycle/remediations "" 200 "GET /api/lifecycle/remediations (list)"
api GET /api/lifecycle/rolling-updates "" 200 "GET /api/lifecycle/rolling-updates (list)"

# ─── Certificates ─────────────────────────────────────────────────────────────

section "Certificates"
api GET /api/certificates/cas "" 200 "GET /api/certificates/cas (list)"
api GET /api/certificates "" 200 "GET /api/certificates (list)"
api GET /api/certificates/expiring "" 200 "GET /api/certificates/expiring (list)"
api GET /api/certificates/requests "" 200 "GET /api/certificates/requests (list)"
api GET /api/certificates/attestations "" 200 "GET /api/certificates/attestations (list)"
api GET /api/certificates/security-baselines "" 200 "GET /api/certificates/security-baselines (list)"
api GET /api/certificates/health "" 200 "GET /api/certificates/health (dashboard)"

# ─── Zones & Spot Instances ───────────────────────────────────────────────────

section "Zones & Spot Instances"
api GET /api/zones "" 200 "GET /api/zones (list)"
api GET /api/spot-instances "" 200 "GET /api/spot-instances (list)"

Z=$(post_body /api/zones '{"name":"e2e-zone","description":"test","region":"us-east"}')
Z_ID=$(extract_id "$Z")

if [ -n "$Z_ID" ]; then
  pass "POST /api/zones (create)"
  api GET "/api/zones/$Z_ID" "" 200 "GET /api/zones/:id"
  api DELETE "/api/zones/$Z_ID" "" 204 "DELETE /api/zones/:id"
else
  fail "POST /api/zones" "201" "no id"
fi

# ─── Autoscale ────────────────────────────────────────────────────────────────

section "Autoscale"
api GET /api/autoscale "" 200 "GET /api/autoscale (list policies)"
api GET /api/autoscale/events "" 200 "GET /api/autoscale/events (list)"

# ─── Events ───────────────────────────────────────────────────────────────────

section "Events"
api GET /api/events "" 200 "GET /api/events (list)"

# ─── Settings ─────────────────────────────────────────────────────────────────

section "Settings"
api GET /api/settings "" 200 "GET /api/settings"

# ─── FluxVM catch-up (stub) ───────────────────────────────────────────────────

# shellcheck source=../scripts/ci-feat-catchup-stub.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/../scripts/ci-feat-catchup-stub.sh"

# ─── Summary ──────────────────────────────────────────────────────────────────

TOTAL=$((PASS + FAIL))
printf "\n\033[1m━━━ Results ━━━\033[0m\n"
printf "  Total: %d  " "$TOTAL"
printf "\033[32mPassed: %d\033[0m  " "$PASS"
if [ "$FAIL" -gt 0 ]; then
  printf "\033[31mFailed: %d\033[0m\n" "$FAIL"
  printf "\n\033[31mFailed tests:\033[0m%b\n" "$ERRORS"
  exit 1
else
  printf "\033[32mFailed: 0\033[0m\n"
  printf "\n\033[32mAll tests passed!\033[0m\n"
fi
