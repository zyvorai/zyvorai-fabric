#!/usr/bin/env bash
# Deprecated name: this script is now scripts/merion-pilot-gate.sh. Kept so existing callers keep working.
echo "note: scripts/keep-pilot-gate.sh is now scripts/merion-pilot-gate.sh" >&2
exec "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/merion-pilot-gate.sh" "$@"
