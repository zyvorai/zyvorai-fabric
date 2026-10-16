#!/usr/bin/env bash
# Deprecated name: this script is now scripts/merion-bake-browser-smoke.sh. Kept so existing callers keep working.
echo "note: scripts/keep-bake-browser-smoke.sh is now scripts/merion-bake-browser-smoke.sh" >&2
exec "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/merion-bake-browser-smoke.sh" "$@"
