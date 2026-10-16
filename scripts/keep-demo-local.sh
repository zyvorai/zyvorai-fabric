#!/usr/bin/env bash
# Deprecated name: this script is now scripts/merion-demo-local.sh. Kept so existing callers keep working.
echo "note: scripts/keep-demo-local.sh is now scripts/merion-demo-local.sh" >&2
exec "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/merion-demo-local.sh" "$@"
