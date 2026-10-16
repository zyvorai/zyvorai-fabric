#!/usr/bin/env bash
# Deprecated name: this script is now scripts/merion-watch.sh. Kept so existing callers keep working.
echo "note: scripts/keep-watch.sh is now scripts/merion-watch.sh" >&2
exec "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/merion-watch.sh" "$@"
