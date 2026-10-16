#!/usr/bin/env bash
# Deprecated name: this script is now scripts/merion-demo-google.sh. Kept so existing callers keep working.
echo "note: scripts/keep-demo-google.sh is now scripts/merion-demo-google.sh" >&2
exec "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/merion-demo-google.sh" "$@"
