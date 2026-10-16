#!/usr/bin/env bash
# Deprecated name: this script is now scripts/merion-live-lab.sh. Kept so existing callers keep working.
echo "note: scripts/keep-live-lab.sh is now scripts/merion-live-lab.sh" >&2
exec "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/merion-live-lab.sh" "$@"
