#!/usr/bin/env bash
# Deprecated name: this script is now scripts/merion-live-tenancy.sh. Kept so existing callers keep working.
echo "note: scripts/keep-live-tenancy.sh is now scripts/merion-live-tenancy.sh" >&2
exec "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/merion-live-tenancy.sh" "$@"
