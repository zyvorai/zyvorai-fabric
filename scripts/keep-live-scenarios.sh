#!/usr/bin/env bash
# Deprecated name: this script is now scripts/merion-live-scenarios.sh. Kept so existing callers keep working.
echo "note: scripts/keep-live-scenarios.sh is now scripts/merion-live-scenarios.sh" >&2
exec "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/merion-live-scenarios.sh" "$@"
