#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0
# Audit dashboard GET endpoints — fail on HTML SPA fallthrough (ghost routes).
set -euo pipefail

HOST="${1:-http://127.0.0.1:9095}"
USER="${FABRIC_USER:-sus}"
PASS="${FABRIC_PASS:-max}"

CURL_OPTS=(-sS)
[[ "$HOST" == https://* ]] && CURL_OPTS+=(-k)

login() {
  curl "${CURL_OPTS[@]}" -f -X POST "${HOST}/api/auth/login" \
    -H 'Content-Type: application/json' \
    -d "{\"username\":\"${USER}\",\"password\":\"${PASS}\"}" \
    | python3 -c 'import sys,json; print(json.load(sys.stdin)["token"])'
}

TOKEN="$(login)"
AUTH=(-H "Authorization: Bearer ${TOKEN}")

ENDPOINTS=(
  /health
  /api/vms
  /api/vms/compare
  /api/users
  /api/webhooks
  /api/services/map
  /api/network/topology
  /api/migrations/history
  /api/migrations/readiness
  /api/migrations/report
  /api/images
  /api/images/iso
  /api/isos
  /api/system/info
  /api/system/metrics
  /api/system/kernel
  /api/system/containers
  /api/system/security
  /api/system/alerts
  /api/system/alerts/rules
  /api/system/compliance
  /api/system/processes
  /api/system/memory
  /api/system/debug/top
  /api/system/explain/cpu
  /api/system/timeseries
  /api/jobs
  /api/pipeline/jobs
  /api/compliance/results
  /api/audit/logs
  /api/ft/vms
  /api/certificates/health
  /api/certificates
  /api/billing/pricing
  /api/billing/usage
  /api/networkd/links
  /api/networkd/bridges
  /api/networkd/bonds
  /api/networkd/vlans
  /api/networkd/macvtaps
  /api/networkd/taps
  /api/networkd/network-files
  /api/networkd/link-files
  /api/networkd/port-forwards
  /api/networkd/vxlans
  /api/networkd/sriov
  /api/services
  /api/nat-rules
  /api/nat-pools
  /api/nat-gateways
  /api/firewall-profiles
  /api/firewall-zones
  /api/firewall-assignments
  /api/qos-policies
  /api/dns-zones
  /api/dns-policies
  /api/vpn-tunnels
  /api/vpn-networks
  /api/mirror-sessions
  /api/monitor-policies
  /api/network-policies
  /api/network-policies/status
  /api/identities
  /api/services/status
  /api/qos-policies/status
  /api/monitor-policies/status
  /api/firewall/status
  /api/nat-rules/status
  /api/mirror-sessions/status
  /api/vpn-tunnels/status
  /api/drs/recommendations/default
  /api/webhooks/deliveries
  /api/events
  /api/templates
  /api/migrations
  /api/backups
  /api/schedules
  /api/profiles
  /api/zones
  /api/floating-ips
  /api/config/snapshot
  /api/events/retention
)

FAIL=0
for path in "${ENDPOINTS[@]}"; do
  code="$(curl "${CURL_OPTS[@]}" -o /tmp/audit-body.txt -w '%{http_code}' "${AUTH[@]}" "${HOST}${path}" 2>/dev/null || echo '000')"
  body="$(cat /tmp/audit-body.txt 2>/dev/null || true)"
  if [[ "$code" == "000" ]]; then
    echo "HTTP_ERROR $path"
    FAIL=$((FAIL + 1))
    continue
  fi
  trimmed="${body#"${body%%[![:space:]]*}"}"
  if [[ "$trimmed" == '<!'* ]] || [[ "$trimmed" == '<html'* ]]; then
    echo "BROKEN_HTML $path (HTTP $code)"
    FAIL=$((FAIL + 1))
  elif [[ "$trimmed" == '{'* ]] || [[ "$trimmed" == '['* ]]; then
    echo "OK_JSON   $path (HTTP $code)"
  elif [[ "$code" =~ ^[45] ]]; then
    echo "HTTP_${code}  $path"
    FAIL=$((FAIL + 1))
  else
    echo "OK_JSON   $path (HTTP $code)"
  fi
done
rm -f /tmp/audit-body.txt

if [[ "$FAIL" -gt 0 ]]; then
  echo "FAILED: $FAIL endpoint(s)" >&2
  exit 1
fi
echo "All ${#ENDPOINTS[@]} endpoints returned JSON"
