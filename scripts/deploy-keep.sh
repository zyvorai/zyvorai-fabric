#!/usr/bin/env bash
# Deprecated name: this script is now scripts/deploy-merion.sh. Kept so existing callers keep working.
echo "note: scripts/deploy-keep.sh is now scripts/deploy-merion.sh" >&2
exec "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/deploy-merion.sh" "$@"
