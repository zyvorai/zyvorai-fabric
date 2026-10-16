#!/usr/bin/env bash
# Deprecated name: this script is now scripts/merion-record-demo.sh. Kept so existing callers keep working.
echo "note: scripts/keep-record-demo.sh is now scripts/merion-record-demo.sh" >&2
exec "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/merion-record-demo.sh" "$@"
