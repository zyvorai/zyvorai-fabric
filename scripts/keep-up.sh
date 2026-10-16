#!/usr/bin/env bash
# Deprecated name: this script is now scripts/merion-up.sh. Kept so existing callers keep working.
echo "note: scripts/keep-up.sh is now scripts/merion-up.sh" >&2
exec "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/merion-up.sh" "$@"
