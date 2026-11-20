#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

# Render docs/social HTML cards with headless Chrome + macOS sips.
#   ./docs/social/build-social-card.sh
# Writes:
#   docs/social/fabric-share-card.png     (1200x630 — README + site OG, light)
#   docs/social/fabric-share-card-dark.png (1200x630 — README dark theme; NOT copied to the site)
#   docs/social/fabric-social-card.jpg    (1600x900 — LinkedIn / X)
#   website/static/img/social-card.png   (byte copy of the share PNG)
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
CHROME="${CHROME:-/Applications/Google Chrome.app/Contents/MacOS/Google Chrome}"
[[ -x "$CHROME" ]] || { echo "Google Chrome not found (set CHROME=...)" >&2; exit 1; }

shot() {
  local html="$1" w="$2" h="$3" out="$4" fmt="$5"
  local raw
  raw="$(mktemp "${TMPDIR:-/tmp}/fabric-card.XXXXXX.png")"
  "$CHROME" --headless=new --disable-gpu --hide-scrollbars --force-device-scale-factor=1 \
    --window-size="${w},${h}" --screenshot="$raw" "file://${html}" >/dev/null 2>&1
  if [[ "$fmt" == jpeg ]]; then
    sips -s format jpeg -s formatOptions 92 "$raw" --out "$out" >/dev/null
  else
    sips -s format png "$raw" --out "$out" >/dev/null
  fi
  rm -f "$raw"
  echo "wrote $out ($(sips -g pixelWidth -g pixelHeight "$out" | awk '/pixel/{printf "%s ", $2}')px, $(du -k "$out" | cut -f1) KB)"
}

shot "$HERE/fabric-share-card.html" 1200 630 "$HERE/fabric-share-card.png" png
shot "$HERE/fabric-share-card-dark.html" 1200 630 "$HERE/fabric-share-card-dark.png" png
shot "$HERE/fabric-social-card.html" 1600 900 "$HERE/fabric-social-card.jpg" jpeg

SITE_OG="$ROOT/website/static/img/social-card.png"
cp "$HERE/fabric-share-card.png" "$SITE_OG"
echo "wrote $SITE_OG (copy of fabric-share-card.png)"
