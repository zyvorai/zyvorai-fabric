#!/usr/bin/env bash
# Deprecated name: this script is now scripts/merion-bake-browser-agent.sh. Kept so existing callers keep working.
echo "note: scripts/keep-bake-browser-agent.sh is now scripts/merion-bake-browser-agent.sh" >&2
exec "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/merion-bake-browser-agent.sh" "$@"
