#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: Apache-2.0
#
# Narrated Keep speculate -> review -> approve demo. It runs against the live Keep runtime that
# scripts/keep-e2e.sh starts (KEEP_E2E_DEMO=scripts/demo/speculate-demo.sh), so every HTTP call
# below is real. FluxVM itself is the CI stub: no VM boots, and the changeset body is a response
# captured from a real FluxVM. The banner says so, because a recording must not imply more.
set -euo pipefail
: "${API:?}" "${TOKEN:?}" "${SID:?}" "${STUB_OPS:?}"

B=$'\e[1m'; D=$'\e[2m'; G=$'\e[32m'; Y=$'\e[33m'; C=$'\e[36m'; R=$'\e[0m'
say()  { printf '%s\n' "$*"; sleep "${PACE:-1.2}"; }
cmd()  { printf '%s$ %s%s\n' "$C" "$*" "$R"; sleep 0.8; }
auth=(-H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json')
ops()  { python3 -c 'import json,sys;print(" -> ".join(json.loads(l)["op"] for l in open(sys.argv[1])) or "(nothing)")' "$STUB_OPS"; }
jq_()  { python3 -c 'import json,sys;d=json.load(sys.stdin);cur=d
for p in sys.argv[1].split("."): cur=cur[p]
print(cur if not isinstance(cur,(dict,list)) else json.dumps(cur))' "$1"; }

say "${B}Keep: an agent proposes a change. A person decides.${R}"
say "${D}real Keep runtime · FluxVM is a test stub (no VM)${R}"
echo
say "${Y}agent wants to run:${R} echo hello > /tmp/specdir/a.txt"
cmd "curl -X POST \$KEEP/v1/sessions/\$SID/speculate -d '{command, paths:[\"/tmp\"]}'"
R1=$(curl -s "${auth[@]}" -X POST "$API/v1/sessions/$SID/speculate" \
  -d '{"command":"echo hello > /tmp/specdir/a.txt","paths":["/tmp"]}')
AID=$(printf '%s' "$R1" | jq_ approval.id)
say "  approval   ${B}$(printf '%s' "$R1" | jq_ approval.status)${R}  kind=$(printf '%s' "$R1" | jq_ approval.kind)"
say "  would add  ${G}$(printf '%s' "$R1" | jq_ approval.planned_action.changes.added)${R}"
say "  egress     $(printf '%s' "$R1" | jq_ approval.planned_action.side_effects.egress)"
echo
say "${B}Nothing has been applied.${R}  FluxVM saw: $(ops)"
echo
say "${Y}the person reviews the diff and approves${R}"
cmd "curl -X POST \$KEEP/v1/approvals/${AID%%-*}… -d '{\"decision\":\"approved\"}'"
R2=$(curl -s "${auth[@]}" -X POST "$API/v1/approvals/$AID" -d '{"decision":"approved"}')
say "  approval   ${G}${B}$(printf '%s' "$R2" | jq_ status)${R}"
say "${B}Now it is applied.${R}  FluxVM saw: $(ops)"
echo
say "${Y}a second proposal is denied${R}"
R3=$(curl -s "${auth[@]}" -X POST "$API/v1/sessions/$SID/speculate" -d '{"command":"rm -rf /tmp/specdir","paths":["/tmp"]}')
AID2=$(printf '%s' "$R3" | jq_ approval.id)
curl -s "${auth[@]}" -X POST "$API/v1/approvals/$AID2" -d '{"decision":"denied"}' >/dev/null
say "${B}Rejected, never applied.${R}  FluxVM saw: $(ops)"
sleep 2
