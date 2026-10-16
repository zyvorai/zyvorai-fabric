#!/usr/bin/env bash
# Deprecated name: this script is now scripts/merion-bake-node22-agent.sh. Kept so existing callers keep working.
echo "note: scripts/keep-bake-node22-agent.sh is now scripts/merion-bake-node22-agent.sh" >&2
exec "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/merion-bake-node22-agent.sh" "$@"
