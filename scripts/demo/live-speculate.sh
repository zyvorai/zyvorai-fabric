#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: Apache-2.0
#
# A real Keep speculate -> approve / deny run against a real FluxVM microVM. Run it ON a host that
# has a Keep runtime (KEEP, TOKEN, SID) and a throwaway FluxVM (FLUX, VM) with /work in the guest.
# Every number printed is measured by this script. Nothing here is scripted output.
set -uo pipefail
: "${KEEP:?}" "${TOKEN:?}" "${SID:?}" "${FLUX:?}" "${VM:?}"
B=$'\e[1m'; D=$'\e[2m'; G=$'\e[32m'; Y=$'\e[33m'; C=$'\e[36m'; R=$'\e[0m'
auth=(-H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json')
cmd()   { printf '%s$ %s%s\n' "$C" "$*" "$R"; }
secs()  { python3 -c "import sys;print(round(float(sys.argv[2])-float(sys.argv[1])))" "$1" "$2"; }
field() { python3 -c 'import json,sys;d=json.load(sys.stdin);cur=d
for p in sys.argv[1].split("."): cur=cur[p]
print(cur if not isinstance(cur,(dict,list)) else json.dumps(cur))' "$1"; }
guest() { for _ in 1 2 3 4 5 6; do r=$(curl -s -m30 -X POST "$FLUX/v1/sandboxes/$VM/process" -H 'Content-Type: application/json' \
          -d "{\"command\":\"$1\",\"timeout_seconds\":10}"); case "$r" in *'"stdout"'*) printf '%s' "$r" | field stdout; return;; esac; sleep 5; done; echo "(guest agent busy)"; }

echo "${B}Keep + a real FluxVM microVM: an agent proposes, a person decides.${R}"
echo "${D}real run on a lab host; the recording trims the waits${R}"
echo "${Y}guest /work now:${R} $(guest 'ls /work' | tr '\n' ' ')"
echo
echo "${Y}agent proposes:${R} echo hello > /work/notes.txt"
cmd "POST /v1/sessions/\$SID/speculate"
t0=$(date +%s.%N)
R1=$(curl -s -m170 "${auth[@]}" -X POST "$KEEP/v1/sessions/$SID/speculate" -d '{"command":"echo hello > /work/notes.txt","paths":["/work"]}')
t1=$(date +%s.%N)
AID=$(printf '%s' "$R1" | field approval.id)
echo "  approval  ${B}$(printf '%s' "$R1" | field approval.status)${R}  would add ${G}$(printf '%s' "$R1" | field approval.planned_action.changes.added)${R}  ${D}(took $(secs "$t0" "$t1") s)${R}"
echo
echo "${Y}person approves${R}"
cmd "POST /v1/approvals/${AID%%-*}…  {decision: approved}"
t2=$(date +%s.%N)
R2=$(curl -s -m175 "${auth[@]}" -X POST "$KEEP/v1/approvals/$AID" -d '{"decision":"approved"}')
t3=$(date +%s.%N)
echo "  approval  ${G}${B}$(printf '%s' "$R2" | field status)${R}  ${D}(applied in $(secs "$t2" "$t3") s)${R}"
echo "${Y}guest /work now:${R} $(guest 'ls /work' | tr '\n' ' ')"
echo "${Y}notes.txt says:${R}  $(guest 'cat /work/notes.txt')"
echo
echo "${Y}agent proposes:${R} rm /work/seed.txt"
R3=$(curl -s -m170 "${auth[@]}" -X POST "$KEEP/v1/sessions/$SID/speculate" -d '{"command":"rm -f /work/seed.txt","paths":["/work"]}')
AID2=$(printf '%s' "$R3" | field approval.id)
echo "  approval  ${B}$(printf '%s' "$R3" | field approval.status)${R}  would delete ${G}$(printf '%s' "$R3" | field approval.planned_action.changes.deleted)${R}"
echo "${Y}person denies${R}"
curl -s -m120 "${auth[@]}" -X POST "$KEEP/v1/approvals/$AID2" -d '{"decision":"denied"}' >/dev/null
echo "${B}Rejected. seed.txt is still there:${R} $(guest 'ls /work' | tr '\n' ' ')"
sleep 2
