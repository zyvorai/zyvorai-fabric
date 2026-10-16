#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: Apache-2.0
#
# Bake / register the Keep browser-agent FluxVM template (Debian 12 + Chromium).
#
# Prerequisites on the FluxVM host (as a sudo-capable user):
#   - fluxctl / fluxvm with image build support
#   - musl or host fluxvm-guest-agent binary
#   - Node 20 tarball downloaded (image build has no network for commands)
#   - Optional: pre-packed playwright-core at /tmp/playwright-pack.tgz
#     (npm pack playwright-core@1.49.1 on a networked host)
#
# Usage:
#   ./scripts/merion-bake-browser-agent.sh
#   MERION_BROWSER_GUEST_AGENT=/usr/local/bin/fluxvm-guest-agent \
#   MERION_BROWSER_NODE_TAR=/tmp/node20.tar.xz \
#     ./scripts/merion-bake-browser-agent.sh
#
# Full image bake can take a long time and needs several GiB free. If the
# qcow2 already exists, this script only (re)registers the template and
# refreshes guest unit/driver assets into the template dir for operators.
#
# Pre-rename names still work: KEEP_<X> is used when MERION_<X> is unset.
[ -n "${MERION_BROWSER_ASSETS+x}" ] || [ -z "${KEEP_BROWSER_ASSETS:-}" ] || export MERION_BROWSER_ASSETS="$KEEP_BROWSER_ASSETS"
[ -n "${MERION_BROWSER_GUEST_AGENT+x}" ] || [ -z "${KEEP_BROWSER_GUEST_AGENT:-}" ] || export MERION_BROWSER_GUEST_AGENT="$KEEP_BROWSER_GUEST_AGENT"
[ -n "${MERION_BROWSER_NODE_TAR+x}" ] || [ -z "${KEEP_BROWSER_NODE_TAR:-}" ] || export MERION_BROWSER_NODE_TAR="$KEEP_BROWSER_NODE_TAR"
[ -n "${MERION_BROWSER_OUT+x}" ] || [ -z "${KEEP_BROWSER_OUT:-}" ] || export MERION_BROWSER_OUT="$KEEP_BROWSER_OUT"
[ -n "${MERION_BROWSER_PLAYWRIGHT_TGZ+x}" ] || [ -z "${KEEP_BROWSER_PLAYWRIGHT_TGZ:-}" ] || export MERION_BROWSER_PLAYWRIGHT_TGZ="$KEEP_BROWSER_PLAYWRIGHT_TGZ"
[ -n "${MERION_BROWSER_TEMPLATE_DIR+x}" ] || [ -z "${KEEP_BROWSER_TEMPLATE_DIR:-}" ] || export MERION_BROWSER_TEMPLATE_DIR="$KEEP_BROWSER_TEMPLATE_DIR"
[ -n "${MERION_BROWSER_UNIT+x}" ] || [ -z "${KEEP_BROWSER_UNIT:-}" ] || export MERION_BROWSER_UNIT="$KEEP_BROWSER_UNIT"

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TPL_SRC="$ROOT/agent-runtime/templates/browser-agent"
OUT_IMG="${MERION_BROWSER_OUT:-/var/lib/fluxvm/images/browser-agent.qcow2}"
TDIR="${MERION_BROWSER_TEMPLATE_DIR:-/var/lib/fluxvm/templates/browser-agent}"
NODE_TAR="${MERION_BROWSER_NODE_TAR:-/tmp/node20.tar.xz}"
PW_PACK="${MERION_BROWSER_PLAYWRIGHT_TGZ:-/tmp/playwright-pack.tgz}"
GUEST_AGENT="${MERION_BROWSER_GUEST_AGENT:-}"
UNIT_SRC="${MERION_BROWSER_UNIT:-}"
ASSETS_STAGING="${MERION_BROWSER_ASSETS:-/tmp/browser-agent-assets}"

if [[ -z "$GUEST_AGENT" ]]; then
  for cand in \
    /usr/local/bin/fluxvm-guest-agent \
    /home/sus/fluxvm-build/target/release/fluxvm-guest-agent \
    "$ROOT/../fluxvm/target/release/fluxvm-guest-agent"; do
    if [[ -x "$cand" ]]; then GUEST_AGENT=$cand; break; fi
  done
fi
if [[ -z "$UNIT_SRC" ]]; then
  for cand in \
    /home/sus/fluxvm-build/systemd/fluxvm-guest-agent.service \
    "$ROOT/../fluxvm/systemd/fluxvm-guest-agent.service"; do
    if [[ -f "$cand" ]]; then UNIT_SRC=$cand; break; fi
  done
fi

echo "==> browser-agent bake"
echo "    image:    $OUT_IMG"
echo "    template: $TDIR"
echo "    node tar: $NODE_TAR"
echo "    playwright pack: $PW_PACK"
echo "    agent:    ${GUEST_AGENT:-MISSING}"
echo "    unit:     ${UNIT_SRC:-MISSING}"

stage_assets() {
  rm -rf "$ASSETS_STAGING"
  mkdir -p "$ASSETS_STAGING"
  cp "$TPL_SRC/driver.mjs" "$TPL_SRC/example.mjs" "$TPL_SRC/package.json" "$ASSETS_STAGING/"
  cp "$TPL_SRC/chromium-cdp.service" "$TPL_SRC/browser-driver.service" "$ASSETS_STAGING/"
}

stage_assets

if [[ ! -f "$OUT_IMG" ]]; then
  [[ -f "$NODE_TAR" ]] || {
    echo "missing $NODE_TAR — download first:" >&2
    echo "  curl -fsSL -o $NODE_TAR https://nodejs.org/dist/v20.18.1/node-v20.18.1-linux-x64.tar.xz" >&2
    exit 1
  }
  [[ -x "$GUEST_AGENT" ]] || { echo "missing fluxvm-guest-agent binary" >&2; exit 1; }
  [[ -f "$UNIT_SRC" ]] || { echo "missing fluxvm-guest-agent.service" >&2; exit 1; }
  command -v fluxctl >/dev/null || command -v fluxvm >/dev/null || {
    echo "fluxctl/fluxvm not on PATH" >&2
    exit 1
  }
  if [[ ! -f "$PW_PACK" ]]; then
    echo "NOTE: $PW_PACK missing — packing playwright-core from npm if network available"
    if command -v npm >/dev/null; then
      TMPPW=$(mktemp -d)
      (cd "$TMPPW" && npm pack playwright-core@1.49.1 >/dev/null)
      mv "$TMPPW"/playwright-core-*.tgz "$PW_PACK"
      rm -rf "$TMPPW"
    else
      echo "missing $PW_PACK and npm — create with: npm pack playwright-core@1.49.1" >&2
      exit 1
    fi
  fi

  BUILD=$(mktemp)
  trap 'rm -f "$BUILD"' EXIT
  python3 - "$TPL_SRC/build.json" "$OUT_IMG" "$NODE_TAR" "$GUEST_AGENT" "$UNIT_SRC" \
    "$ASSETS_STAGING" "$PW_PACK" "$BUILD" <<'PY'
import json, sys
src, out, node, agent, unit, assets, pw, dest = sys.argv[1:]
spec = json.load(open(src))
spec["output"] = out
spec["copy_in"] = [
  {"src": node, "dest": "/root/node20.tar.xz"},
  {"src": agent, "dest": "/usr/local/bin/fluxvm-guest-agent"},
  {"src": unit, "dest": "/etc/systemd/system/fluxvm-guest-agent.service"},
  {"src": assets, "dest": "/root/browser-assets"},
  {"src": pw, "dest": "/root/playwright-pack.tgz"},
]
json.dump(spec, open(dest, "w"), indent=2)
print("wrote", dest)
PY
  echo "==> building image (this may take a long time)"
  if command -v fluxctl >/dev/null; then
    sudo fluxctl --config /etc/fluxvm.toml build-image --spec "$BUILD"
  else
    sudo fluxvm --config /etc/fluxvm.toml build-image --spec "$BUILD"
  fi
else
  echo "==> image already present — skip build"
fi

echo "==> registering template"
sudo mkdir -p "$TDIR"
sudo cp "$TPL_SRC/spec.json" "$TDIR/spec.json"
sudo python3 - <<PY
import json
p="$TDIR/spec.json"
d=json.load(open(p))
d["image"]="$OUT_IMG"
json.dump(d, open(p,"w"), indent=2)
print("registered", p)
PY

echo "OK — browser-agent template ready"
echo "    Manifest: template=browser-agent browser_port=9222 confinement=strict"
echo "    Guest: Chromium CDP 127.0.0.1:9222 + driver 127.0.0.1:9230"
echo "    Gate: curl --noproxy '*' https://example.com must FAIL inside the cell"
