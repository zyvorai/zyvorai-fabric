#!/usr/bin/env bash
# scripts/merion-bake-node22-agent.sh: --force must build a NEW image beside the old one (an in-place rebuild fails on a locked image),
# re-point the template, keep the previous spec for rollback, and never touch the old image. Runs with fake fluxctl and no root.
# Pre-rename names still work: KEEP_<X> is used when MERION_<X> is unset.
[ -n "${MERION_NODE_GUEST_AGENT+x}" ] || [ -z "${KEEP_NODE_GUEST_AGENT:-}" ] || export MERION_NODE_GUEST_AGENT="$KEEP_NODE_GUEST_AGENT"
[ -n "${MERION_NODE_IMAGES_DIR+x}" ] || [ -z "${KEEP_NODE_IMAGES_DIR:-}" ] || export MERION_NODE_IMAGES_DIR="$KEEP_NODE_IMAGES_DIR"
[ -n "${MERION_NODE_TAR+x}" ] || [ -z "${KEEP_NODE_TAR:-}" ] || export MERION_NODE_TAR="$KEEP_NODE_TAR"
[ -n "${MERION_NODE_TEMPLATE_DIR+x}" ] || [ -z "${KEEP_NODE_TEMPLATE_DIR:-}" ] || export MERION_NODE_TEMPLATE_DIR="$KEEP_NODE_TEMPLATE_DIR"
[ -n "${MERION_SUDO+x}" ] || [ -z "${KEEP_SUDO:-}" ] || export MERION_SUDO="$KEEP_SUDO"

set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
W="$(mktemp -d)"; trap 'rm -rf "$W"' EXIT
fail() { echo "bake-fresh-image: FAIL: $*" >&2; exit 1; }
ok() { echo "  ok  $*"; }
mkdir -p "$W/bin" "$W/images" "$W/tmpl"
printf '#!/bin/sh\nexit 0\n' > "$W/agent"; chmod +x "$W/agent"
echo node > "$W/node.tar.xz"
# a fake builder: writes the spec's output file and logs where it was asked to write
cat > "$W/bin/fluxctl" <<'SH'
#!/usr/bin/env bash
spec=""; while [[ $# -gt 0 ]]; do [[ "$1" == "--spec" ]] && spec="$2"; shift; done
out=$(python3 -c "import json,sys; print(json.load(open(sys.argv[1]))['output'])" "$spec")
echo "$out" >> "$FAKE_LOG"
[[ -e "$out.locked" ]] && { echo 'qemu-img: Failed to get "write" lock' >&2; exit 1; }
echo "built-$(date +%s%N)" > "$out"
SH
chmod +x "$W/bin/fluxctl"
export PATH="$W/bin:$PATH" FAKE_LOG="$W/calls.log" MERION_SUDO="" MERION_NODE_IMAGES_DIR="$W/images" MERION_NODE_TEMPLATE_DIR="$W/tmpl" \
  MERION_NODE_TAR="$W/node.tar.xz" MERION_NODE_GUEST_AGENT="$W/agent"
bake() { "$ROOT/scripts/merion-bake-node22-agent.sh" "$@" 2>&1; }
image_of() { python3 -c "import json,sys; print(json.load(open(sys.argv[1]))['image'])" "$1"; }

bake >"$W/o1" || fail "first bake: $(cat "$W/o1")"
[[ -f "$W/images/node22-agent.qcow2" ]] || fail "first bake did not write the default image"
[[ "$(image_of "$W/tmpl/spec.json")" == "$W/images/node22-agent.qcow2" ]] || fail "template not pointed at the default image"
[[ ! -e "$W/tmpl/spec.json.prev" ]] || fail "a first bake has nothing to keep as .prev"
ok "first bake builds the default image and registers it"

old_sum=$(cksum < "$W/images/node22-agent.qcow2"); touch "$W/images/node22-agent.qcow2.locked"   # the old image is now 'locked'
: > "$FAKE_LOG"
bake --force >"$W/o2" || fail "--force bake failed even though only the old image is locked: $(cat "$W/o2")"
n=$(ls "$W/images"/node22-agent-*.qcow2 2>/dev/null | wc -l | tr -d ' ')
[[ "$n" == 1 ]] || fail "expected one new timestamped image, found $n"
new=$(ls "$W/images"/node22-agent-*.qcow2)
grep -qxF "$W/images/node22-agent.qcow2" "$FAKE_LOG" && fail "the builder was asked to overwrite the old image in place"
grep -qxF "$new" "$FAKE_LOG" || fail "the builder was not asked to write the new image"
[[ "$(cksum < "$W/images/node22-agent.qcow2")" == "$old_sum" ]] || fail "the old image was modified"
[[ "$(image_of "$W/tmpl/spec.json")" == "$new" ]] || fail "template not re-pointed at the new image"
[[ "$(image_of "$W/tmpl/spec.json.prev")" == "$W/images/node22-agent.qcow2" ]] || fail "spec.json.prev does not point at the old image"
grep -q "Roll back" "$W/o2" || fail "rollback hint missing"
ok "--force builds beside the locked image, re-points the template, keeps spec.json.prev and leaves the old image untouched"

: > "$FAKE_LOG"; before=$(ls "$W/images" | wc -l | tr -d ' ')
bake >"$W/o3" || fail "plain re-run failed"
[[ ! -s "$FAKE_LOG" ]] || fail "a plain re-run must not rebuild"
ok "a plain re-run does not rebuild"
echo "bake-fresh-image: 3 checks passed"
