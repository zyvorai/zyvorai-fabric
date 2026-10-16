#!/usr/bin/env bash
# Deprecated name: this script is now scripts/merion-bake-fc-rootfs.sh. Kept so existing callers keep working.
echo "note: scripts/keep-bake-fc-rootfs.sh is now scripts/merion-bake-fc-rootfs.sh" >&2
exec "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/merion-bake-fc-rootfs.sh" "$@"
