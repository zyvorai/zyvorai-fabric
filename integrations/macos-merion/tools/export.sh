#!/usr/bin/env bash
# Copy this folder into a checkout of the standalone Solvor repository (a snapshot: fabric stays the source of truth).
#   tools/export.sh ../solvor        then review `git status` there, commit and push
set -euo pipefail
DEST="${1:?usage: tools/export.sh path/to/solvor-checkout}"
HERE="$(cd "$(dirname "$0")/.." && pwd)"
[[ -d "$DEST" ]] || { echo "$DEST is not a directory" >&2; exit 1; }
rsync -a --delete --exclude '/.git' --exclude '/build' --exclude '/.build' --exclude '/.swiftpm' --exclude 'xcuserdata' --exclude '.DS_Store' \
  --exclude '/.gitignore' --exclude '/LICENSE' "$HERE/" "$DEST/"
cp "$HERE/../../LICENSE" "$DEST/LICENSE"
[[ -f "$DEST/.gitignore" ]] || printf '.build/\n.swiftpm/\nbuild/\nxcuserdata/\n*.xcuserstate\n.DS_Store\n' > "$DEST/.gitignore"
echo "exported to $DEST"
