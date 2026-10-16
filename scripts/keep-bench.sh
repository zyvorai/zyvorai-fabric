#!/usr/bin/env bash
# Deprecated name: this script is now scripts/merion-bench.sh. Kept so existing callers keep working.
echo "note: scripts/keep-bench.sh is now scripts/merion-bench.sh" >&2
exec "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/merion-bench.sh" "$@"
