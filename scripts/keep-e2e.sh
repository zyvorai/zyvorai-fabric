#!/usr/bin/env bash
# Deprecated name: this script is now scripts/merion-e2e.sh. Kept so existing callers keep working.
echo "note: scripts/keep-e2e.sh is now scripts/merion-e2e.sh" >&2
exec "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/merion-e2e.sh" "$@"
