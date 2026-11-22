#!/bin/bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0
# ============================================================================
# build.sh — Build zyvor-fabricd backend and web dashboard
# ============================================================================
# Usage:
#   ./build.sh                  # Build everything (backend + web)
#   ./build.sh --backend        # Backend only
#   ./build.sh --web            # Web only
#   ./build.sh --release        # Release build (default)
#   ./build.sh --debug          # Debug build
#   ./build.sh --check          # Compile check only (no binary output)
# ============================================================================

set -euo pipefail

info()    { echo "  ✅ $*"; }
warn()    { echo "  ⚠️  $*"; }
err()     { echo "  ❌ $*"; exit 1; }
step()    { echo ""; echo "  🔧 $*"; }

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

BUILD_BACKEND=true
BUILD_WEB=true
BUILD_MODE="release"
CHECK_ONLY=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --backend)  BUILD_WEB=false; shift ;;
        --web)      BUILD_BACKEND=false; shift ;;
        --release)  BUILD_MODE="release"; shift ;;
        --debug)    BUILD_MODE="debug"; shift ;;
        --check)    CHECK_ONLY=true; shift ;;
        --help|-h)
            echo "Usage: $0 [--backend|--web] [--release|--debug|--check]"
            exit 0
            ;;
        *) err "Unknown option: $1" ;;
    esac
done

START_TIME=$(date +%s)

echo ""
echo "  ╔══════════════════════════════════════════════════╗"
echo "  ║     🔨 zyvor-fabricd Build                            ║"
echo "  ╚══════════════════════════════════════════════════╝"
echo ""

if $BUILD_BACKEND; then
    if $CHECK_ONLY; then
        step "Checking backend (cargo check)"
        cd backend
        cargo check 2>&1
        info "Backend check passed — zero errors"
        cd "$SCRIPT_DIR"
    else
        step "Building backend (${BUILD_MODE})"
        cd backend
        if [[ "$BUILD_MODE" == "release" ]]; then
            cargo build --release 2>&1
            for bin in zyvor-fabricd fabricctl; do
                if [[ -f "target/release/$bin" ]]; then
                    info "$bin ($(du -h "target/release/$bin" | cut -f1))"
                fi
            done
        else
            cargo build 2>&1
            for bin in zyvor-fabricd fabricctl; do
                if [[ -f "target/debug/$bin" ]]; then
                    info "$bin (debug, $(du -h "target/debug/$bin" | cut -f1))"
                fi
            done
        fi
        cd "$SCRIPT_DIR"
    fi
fi

if $BUILD_WEB; then
    step "Building web dashboard"
    cd web
    if [[ -f "package.json" ]]; then
        npm install --silent 2>&1 | tail -1
        npm run build 2>&1 | tail -5
        if [[ -d "dist" ]]; then
            FILE_COUNT=$(find dist -type f | wc -l)
            TOTAL_SIZE=$(du -sh dist | cut -f1)
            info "Dashboard built: ${FILE_COUNT} files, ${TOTAL_SIZE}"
        fi
    else
        warn "package.json not found — skipping"
    fi
    cd "$SCRIPT_DIR"
fi

END_TIME=$(date +%s)
ELAPSED=$((END_TIME - START_TIME))

echo ""
echo "  ════════════════════════════════════════════════════"
echo "  🎉 Build complete in ${ELAPSED}s"
echo "  ════════════════════════════════════════════════════"
echo ""
