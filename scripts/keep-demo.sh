#!/usr/bin/env bash
# Deprecated name: this script is now scripts/merion-demo.sh. Kept so existing callers keep working.
echo "note: scripts/keep-demo.sh is now scripts/merion-demo.sh" >&2
exec "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/merion-demo.sh" "$@"
