#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: Apache-2.0
#
# Keep scoreboard gate. No KVM required.
#
# This is a wiring/self-consistency check, not a live measurement of a real runtime: it writes a
# policy, packs it, unpacks it onto a second directory, and hashes it, then writes a scoreboard
# JSON that asserts on values (including the two model sockets) that this same script just wrote.
# It proves the receipt shape and the pack-round-trip mechanics agree with agent-runtime/src/
# scoreboard.rs's validation rules; it does not exercise the real egress broker, a real deployed
# agent's actual model socket configuration, or a real cell. See docs/merion/AHEAD.md.
# Pre-rename names still work: KEEP_<X> is used when MERION_<X> is unset.
[ -n "${MERION_SCOREBOARD_OUT+x}" ] || [ -z "${KEEP_SCOREBOARD_OUT:-}" ] || export MERION_SCOREBOARD_OUT="$KEEP_SCOREBOARD_OUT"

set -euo pipefail
cd "$(dirname "$0")/.."

OUT="${MERION_SCOREBOARD_OUT:-artifacts/keep-scoreboard.json}"
mkdir -p "$(dirname "$OUT")"

POLICY=$(cat <<'YAML'
schema: keep.policy/v1
training: off
egress:
  default: deny
  allow: []
approvals:
  buy: out-of-band
  send: out-of-band
  delete: out-of-band
model_socket: socket://lab/a
YAML
)

tmpdir=$(mktemp -d)
trap 'rm -rf "$tmpdir"' EXIT
printf '%s\n' "$POLICY" > "$tmpdir/keep.policy.yaml"
printf '%s\n' '{"name":"pdf-brief","connect":0}' > "$tmpdir/agent.json"
printf '%s\n' 'move policy and manifest; leave vault behind' > "$tmpdir/MIGRATION.md"

# pack
mkdir -p "$tmpdir/pack"
cp "$tmpdir/keep.policy.yaml" "$tmpdir/agent.json" "$tmpdir/MIGRATION.md" "$tmpdir/pack/"
if find "$tmpdir/pack" -iname '*vault*' -o -iname '*secret*' -o -iname '*credential*' | grep -q .; then
  echo "scoreboard: secret leaked into pack" >&2
  exit 1
fi
# unpack onto a second "node"
mkdir -p "$tmpdir/node-b"
cp -a "$tmpdir/pack/." "$tmpdir/node-b/"
cmp "$tmpdir/keep.policy.yaml" "$tmpdir/node-b/keep.policy.yaml"

sha=$(sha256sum "$tmpdir/keep.policy.yaml" | awk '{print $1}')

cat > "$OUT" <<EOF
{
  "schema": "keep.scoreboard/v1",
  "policy_sha256": "${sha}",
  "evidence_class": "software-test",
  "egress_connects": 0,
  "session_frozen": false,
  "training_default": "off",
  "model_sockets": ["socket://lab/a", "socket://lab/b"],
  "pack_roundtrip": true,
  "secrets_in_pack": false,
  "snp_launch_verified": false,
  "tdx_launch_verified": false,
  "operator_can_read": true,
  "host_recover_allowed": true,
  "claim": "software-test measurement; host can read the cell"
}
EOF

python3 - "$OUT" <<'PY'
import json, sys
r = json.load(open(sys.argv[1]))
assert r["schema"] == "keep.scoreboard/v1"
assert len(r["policy_sha256"]) == 64
assert r["egress_connects"] == 0
assert r["training_default"] == "off"
assert r["secrets_in_pack"] is False
assert r["pack_roundtrip"] is True
assert r["snp_launch_verified"] is False and r["tdx_launch_verified"] is False
assert r["operator_can_read"] is True
assert len(set(r["model_sockets"])) == 2
print("keep-scoreboard: ok", r["policy_sha256"][:12])
PY

echo "wrote $OUT"
