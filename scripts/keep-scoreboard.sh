#!/usr/bin/env bash
# Deprecated name: this script is now scripts/merion-scoreboard.sh. Kept so existing callers keep working.
echo "note: scripts/keep-scoreboard.sh is now scripts/merion-scoreboard.sh" >&2
exec "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/merion-scoreboard.sh" "$@"
