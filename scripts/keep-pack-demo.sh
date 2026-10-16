#!/usr/bin/env bash
# Deprecated name: this script is now scripts/merion-pack-demo.sh. Kept so existing callers keep working.
echo "note: scripts/keep-pack-demo.sh is now scripts/merion-pack-demo.sh" >&2
exec "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/merion-pack-demo.sh" "$@"
