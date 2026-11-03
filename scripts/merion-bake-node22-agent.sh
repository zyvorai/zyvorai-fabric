#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: Apache-2.0
#
# Bake and register the node22-agent FluxVM template (Ubuntu 24.04 + Node 22 + poppler + tesseract + guest agent):
# the cell every Keep use case runs in.
#
# Run on the FluxVM host, as a user with sudo:
#   ./scripts/merion-bake-node22-agent.sh              # download Node, build the image, register the template
#   ./scripts/merion-bake-node22-agent.sh --dry-run    # print the plan and what is missing; change nothing
#   ./scripts/merion-bake-node22-agent.sh --force      # rebuild into a NEW image next to the old one and point the template at it
#
# Environment overrides are listed in agent-runtime/templates/node22-agent/README.md.
# Pre-rename names still work: KEEP_<X> is used when MERION_<X> is unset.
[ -n "${MERION_BAKE_STAGE_DIR+x}" ] || [ -z "${KEEP_BAKE_STAGE_DIR:-}" ] || export MERION_BAKE_STAGE_DIR="$KEEP_BAKE_STAGE_DIR"
[ -n "${MERION_FLUXVM_CONFIG+x}" ] || [ -z "${KEEP_FLUXVM_CONFIG:-}" ] || export MERION_FLUXVM_CONFIG="$KEEP_FLUXVM_CONFIG"
[ -n "${MERION_NODE_BASE_IMG+x}" ] || [ -z "${KEEP_NODE_BASE_IMG:-}" ] || export MERION_NODE_BASE_IMG="$KEEP_NODE_BASE_IMG"
[ -n "${MERION_NODE_GUEST_AGENT+x}" ] || [ -z "${KEEP_NODE_GUEST_AGENT:-}" ] || export MERION_NODE_GUEST_AGENT="$KEEP_NODE_GUEST_AGENT"
[ -n "${MERION_NODE_IMAGES_DIR+x}" ] || [ -z "${KEEP_NODE_IMAGES_DIR:-}" ] || export MERION_NODE_IMAGES_DIR="$KEEP_NODE_IMAGES_DIR"
[ -n "${MERION_NODE_OUT+x}" ] || [ -z "${KEEP_NODE_OUT:-}" ] || export MERION_NODE_OUT="$KEEP_NODE_OUT"
[ -n "${MERION_NODE_TAR+x}" ] || [ -z "${KEEP_NODE_TAR:-}" ] || export MERION_NODE_TAR="$KEEP_NODE_TAR"
[ -n "${MERION_NODE_TEMPLATE_DIR+x}" ] || [ -z "${KEEP_NODE_TEMPLATE_DIR:-}" ] || export MERION_NODE_TEMPLATE_DIR="$KEEP_NODE_TEMPLATE_DIR"
[ -n "${MERION_NODE_VERSION+x}" ] || [ -z "${KEEP_NODE_VERSION:-}" ] || export MERION_NODE_VERSION="$KEEP_NODE_VERSION"
[ -n "${MERION_SUDO+x}" ] || [ -z "${KEEP_SUDO:-}" ] || export MERION_SUDO="$KEEP_SUDO"

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC="$ROOT/agent-runtime/templates/node22-agent"
NODE_VERSION="${MERION_NODE_VERSION:-22.11.0}"
NODE_TAR="${MERION_NODE_TAR:-/tmp/node.tar.xz}"
NODE_URL="https://nodejs.org/dist/v${NODE_VERSION}/node-v${NODE_VERSION}-linux-x64.tar.xz"
IMG_DIR="${MERION_NODE_IMAGES_DIR:-/var/lib/fluxvm/images}"
OUT_IMG_SET="${MERION_NODE_OUT:-}"
OUT_IMG="${MERION_NODE_OUT:-$IMG_DIR/node22-agent.qcow2}"
TDIR="${MERION_NODE_TEMPLATE_DIR:-/var/lib/fluxvm/templates/node22-agent}"
GUEST_AGENT="${MERION_NODE_GUEST_AGENT:-/usr/local/bin/fluxvm-guest-agent}"
BASE_IMG="${MERION_NODE_BASE_IMG:-}"
FLUX_CONFIG="${MERION_FLUXVM_CONFIG:-/etc/fluxvm.toml}"
# sudo unless already root; MERION_SUDO="" also disables it (used by the tests)
SUDO="${MERION_SUDO-sudo}"
[[ "$(id -u)" == 0 ]] && SUDO=""
DRY=0
FORCE=0
for arg in "$@"; do
  case "$arg" in
    --dry-run) DRY=1 ;;
    --force) FORCE=1 ;;
    -h|--help) sed -n '5,12p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown option: $arg" >&2; exit 64 ;;
  esac
done

# --force never rebuilds in place: FluxVM keeps a running or stale qemu-nbd attached to the old image, and qemu-img then fails with
# 'Failed to get "write" lock'. Build a fresh, timestamped image beside it and re-point the template; the old image stays for rollback.
PREV_IMG=""
if [[ "$FORCE" == 1 && -z "$OUT_IMG_SET" && -f "$OUT_IMG" ]]; then
  PREV_IMG="$OUT_IMG"
  OUT_IMG="${OUT_IMG%.qcow2}-$(date +%Y%m%d%H%M%S).qcow2"
fi

FLUX=""
for c in fluxctl fluxvm; do command -v "$c" >/dev/null 2>&1 && { FLUX=$c; break; }; done

problems=0
need() { echo "  MISSING: $*" >&2; problems=$((problems + 1)); }

echo "==> node22-agent bake"
echo "    image:       $OUT_IMG"
echo "    template:    $TDIR"
echo "    node:        v$NODE_VERSION ($NODE_TAR)"
echo "    guest agent: $GUEST_AGENT"
echo "    base image:  ${BASE_IMG:-$(python3 -c "import json,sys; print(json.load(open(sys.argv[1]))['source'])" "$SRC/build.json")}"
echo "    builder:     ${FLUX:-none}"

[[ -n "$FLUX" ]] || need "fluxvm / fluxctl on the PATH"
[[ -x "$GUEST_AGENT" ]] || need "guest agent binary at $GUEST_AGENT (set MERION_NODE_GUEST_AGENT)"
[[ -z "$BASE_IMG" || -f "$BASE_IMG" ]] || need "base image $BASE_IMG"
command -v python3 >/dev/null || need "python3"
if [[ -x "$GUEST_AGENT" ]] && command -v ldd >/dev/null 2>&1; then
  if ldd "$GUEST_AGENT" 2>&1 | grep -q "libc.so"; then
    echo "  WARNING: $GUEST_AGENT is dynamically linked. If it needs a newer glibc than the guest has it will fail to start" >&2
    echo "           in the guest ('GLIBC_x.y not found'). Prefer a static (musl) build." >&2
  fi
fi
if [[ ! -f "$NODE_TAR" ]]; then
  echo "    (will download $NODE_URL)"
  command -v curl >/dev/null || need "curl (to fetch Node)"
fi
if [[ -f "$OUT_IMG" && "$FORCE" == 0 ]]; then echo "    image exists: the build will be skipped (use --force to build a new image beside it)"; fi
if [[ -n "$PREV_IMG" ]]; then echo "    --force: a new image will be built at $OUT_IMG; $PREV_IMG is left untouched"; fi

if (( problems > 0 )); then echo "==> $problems problem(s) above" >&2; exit 1; fi
if (( DRY )); then echo "==> dry run: nothing changed"; exit 0; fi

if [[ ! -f "$OUT_IMG" || "$FORCE" == 1 ]]; then
  if [[ ! -f "$NODE_TAR" ]]; then
    echo "==> downloading Node v$NODE_VERSION"
    curl -fSL -o "$NODE_TAR.part" "$NODE_URL"
    mv "$NODE_TAR.part" "$NODE_TAR"
  fi
  # Stage every build input under /var/lib/fluxvm: the AppArmor profile FluxVM's bootstrap installs lets fluxctl read
  # only there (a spec in /tmp or an agent in /usr/local/bin is refused with "Permission denied").
  STAGE_ROOT="${MERION_BAKE_STAGE_DIR:-/var/lib/fluxvm/downloads}"
  if [[ -d "$STAGE_ROOT" ]]; then STAGE="$($SUDO mktemp -d "$STAGE_ROOT/keep-bake.XXXXXX")"; else STAGE="$(mktemp -d)"; fi
  trap '$SUDO rm -rf "$STAGE"' EXIT
  $SUDO cp "$SRC/fluxvm-guest-agent.service" "$STAGE/"
  $SUDO cp "$NODE_TAR" "$STAGE/node.tar.xz"
  $SUDO cp "$GUEST_AGENT" "$STAGE/fluxvm-guest-agent"
  BUILD_TMP="$(mktemp)"
  python3 - "$SRC/build.json" "$BUILD_TMP" "$OUT_IMG" "$STAGE/node.tar.xz" "$STAGE/fluxvm-guest-agent" "$STAGE/fluxvm-guest-agent.service" "$BASE_IMG" "$NODE_VERSION" <<'PY'
import json, sys
src, dest, out, node, agent, unit, base, version = sys.argv[1:]
spec = json.load(open(src))
spec["output"] = out
if base:
    spec["source"] = base
spec["copy_in"] = [
    {"src": node, "dest": "/root/node.tar.xz"},
    {"src": agent, "dest": "/usr/local/bin/fluxvm-guest-agent"},
    {"src": unit, "dest": "/etc/systemd/system/fluxvm-guest-agent.service"},
]
spec["pins"] = {"node": f"v{version}"}
json.dump(spec, open(dest, "w"), indent=2)
PY
  $SUDO cp "$BUILD_TMP" "$STAGE/build.json"; rm -f "$BUILD_TMP"
  BUILD="$STAGE/build.json"
  echo "==> building the image (a few minutes; needs several GiB free)"
  # A failed build leaves a half-written image at $OUT_IMG that the next run would take for a finished one (and
  # register). This path did not exist before this step, so removing it on failure is safe.
  if ! $SUDO "$FLUX" --config "$FLUX_CONFIG" build-image --spec "$BUILD"; then
    $SUDO rm -f "$OUT_IMG"
    echo "the image build failed; the partial image was removed" >&2
    exit 1
  fi
else
  echo "==> image already present: skipping the build"
fi

echo "==> registering the template"
$SUDO mkdir -p "$TDIR"
# keep the spec we are replacing, so a bad image can be rolled back by copying it back
[[ -f "$TDIR/spec.json" ]] && $SUDO cp "$TDIR/spec.json" "$TDIR/spec.json.prev"
$SUDO cp "$SRC/spec.json" "$TDIR/spec.json"
$SUDO python3 - "$TDIR/spec.json" "$OUT_IMG" <<'PY'
import json, sys
path, image = sys.argv[1:]
d = json.load(open(path))
d["image"] = image
json.dump(d, open(path, "w"), indent=2)
print("registered", path)
PY

if command -v curl >/dev/null 2>&1; then
  if curl -fsS -m 5 http://127.0.0.1:7788/v1/templates 2>/dev/null | grep -q '"node22-agent"'; then
    echo "OK: FluxVM lists node22-agent"
  else
    echo "Registered, but FluxVM did not list it yet (restart the control plane if it does not appear):"
    echo "  curl -s http://127.0.0.1:7788/v1/templates"
  fi
fi
if [[ -n "$PREV_IMG" ]]; then
  echo "The previous image is still at $PREV_IMG."
  echo "  Roll back:  sudo cp $TDIR/spec.json.prev $TDIR/spec.json"
  echo "  Clean up:   sudo rm $PREV_IMG    (only once no VM or qemu-nbd uses it: lsof $PREV_IMG)"
fi
echo "Next: ./scripts/merion-demo.sh csv-clean"
