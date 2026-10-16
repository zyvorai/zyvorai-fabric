#!/usr/bin/env bash
# Deprecated name: this script is now scripts/merion-demo-pdf.sh. Kept so existing callers keep working.
echo "note: scripts/keep-demo-pdf.sh is now scripts/merion-demo-pdf.sh" >&2
exec "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/merion-demo-pdf.sh" "$@"
