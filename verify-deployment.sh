#!/bin/bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0
# ============================================================================
# verify-deployment.sh — Quick deployment health check for Zyvor Fabric (zyvor-fabricd)
# ============================================================================
# Lightweight check that can be run after deploy-remote.sh or setup.sh
# to confirm the deployment is healthy.
#
# Usage:
#   ./verify-deployment.sh                    # Check localhost
#   ./verify-deployment.sh 185.165.240.5      # Check remote host
#   ./verify-deployment.sh 10.0.0.1 root pass # Check remote with password
# ============================================================================

set -euo pipefail

HOST="${1:-localhost}"
USER="${2:-root}"
PASS="${3:-}"
PORT="${ZYVOR_FABRICD_PORT:-9095}"

pass() { echo "  ✅ $*"; }
fail() { echo "  ❌ $*"; }
warn() { echo "  ⚠️  $*"; }

_ssh() {
    if [[ "$HOST" == "localhost" ]] || [[ "$HOST" == "127.0.0.1" ]]; then
        bash -c "$*"
    elif [[ -n "$PASS" ]]; then
        SSHPASS="$PASS" sshpass -e ssh -o StrictHostKeyChecking=no "${USER}@${HOST}" "$@"
    else
        ssh -o StrictHostKeyChecking=no "${USER}@${HOST}" "$@"
    fi
}

echo ""
echo "  ╔══════════════════════════════════════════════════╗"
echo "  ║     🩺 Zyvor Fabric Deployment Verification       ║"
echo "  ╚══════════════════════════════════════════════════╝"
echo ""
echo "  Target: ${HOST}"
echo ""

CHECKS=0
PASSED=0

check() {
    ((CHECKS++))
    if "$@"; then
        ((PASSED++))
    fi
}

# ── Service check ──
echo "  ── Services ──"
check _ssh "
    if systemctl is-active zyvor-fabricd &>/dev/null; then
        echo '  ✅ zyvor-fabricd: running'
        true
    else
        echo '  ❌ zyvor-fabricd: not running'
        false
    fi
"

# ── Binary check ──
echo ""
echo "  ── Binaries ──"
for bin in zyvor-fabricd fabricctl; do
    check _ssh "
        if command -v $bin &>/dev/null; then
            echo \"  ✅ $bin: \$(command -v $bin)\"
            true
        else
            echo '  ❌ $bin: not found'
            false
        fi
    "
done

# ── Config check ──
echo ""
echo "  ── Configuration ──"
check _ssh "
    if [ -f /etc/zyvor-fabricd/zyvor-fabricd.toml ]; then
        echo '  ✅ Config: /etc/zyvor-fabricd/zyvor-fabricd.toml'
        true
    else
        echo '  ❌ Config: not found'
        false
    fi
"

# ── Data dirs ──
echo ""
echo "  ── Storage ──"
check _ssh "
    if [ -d /var/lib/zyvor-fabricd ]; then
        SIZE=\$(du -sh /var/lib/zyvor-fabricd 2>/dev/null | cut -f1)
        echo \"  ✅ /var/lib/zyvor-fabricd: \$SIZE\"
        true
    else
        echo '  ❌ /var/lib/zyvor-fabricd: not found'
        false
    fi
"

# ── Dashboard ──
echo ""
echo "  ── Dashboard ──"
check _ssh "
    for dir in /usr/share/zyvor-fabricd/web /usr/local/share/zyvor-fabricd/web; do
        if [ -d \"\$dir\" ] && [ -f \"\$dir/index.html\" ]; then
            COUNT=\$(find \$dir -type f | wc -l)
            echo \"  ✅ Dashboard: \$dir (\$COUNT files)\"
            exit 0
        fi
    done
    echo '  ⚠️  Dashboard: not deployed'
    true
"

# ── API health ──
echo ""
echo "  ── API ──"
if [[ "$HOST" == "localhost" ]] || [[ "$HOST" == "127.0.0.1" ]]; then
    API_HOST="localhost"
else
    API_HOST="$HOST"
fi

HTTP_CODE=$(curl -s -o /dev/null -w '%{http_code}' --connect-timeout 5 "http://${API_HOST}:${PORT}/api/health" 2>/dev/null || echo "000")
if [[ "$HTTP_CODE" == "200" ]]; then
    pass "GET /api/health: 200 OK"
    ((PASSED++))
elif [[ "$HTTP_CODE" == "000" ]]; then
    fail "GET /api/health: connection refused (port ${PORT})"
else
    warn "GET /api/health: HTTP $HTTP_CODE"
fi
((CHECKS++))

# ── Summary ──
echo ""
echo "  ════════════════════════════════════════════════════"
echo "  Results: $PASSED/$CHECKS checks passed"
echo "  ════════════════════════════════════════════════════"
echo ""

if [[ $PASSED -eq $CHECKS ]]; then
    echo "  🎉 Deployment is healthy!"
else
    echo "  ⚠️  Some checks failed. Investigate with:"
    echo "    systemctl status zyvor-fabricd"
    echo "    journalctl -u zyvor-fabricd -n 20"
fi
echo ""

[[ $PASSED -eq $CHECKS ]]
