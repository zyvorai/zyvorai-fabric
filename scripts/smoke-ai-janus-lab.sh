#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

# Lab smoke for the Janus GPU stand-in (Preview).
# Requires fabricd with FLUXVM_AI_JANUS_URL and a healthy Janus NodePort.
# Usage:
#   FABRIC_URL=https://127.0.0.1:9095 ./scripts/smoke-ai-janus-lab.sh
set -euo pipefail

FABRIC_URL="${FABRIC_URL:-https://127.0.0.1:9095}"
JANUS_URL="${JANUS_URL:-http://127.0.0.1:30818}"
USER="${FABRIC_USER:-admin}"
PASS="${FABRIC_PASS:-}"
if [[ -z "$PASS" && -f /var/lib/zyvor-fabricd/.admin_password ]]; then
  PASS="$(sudo cat /var/lib/zyvor-fabricd/.admin_password)"
fi
PASS="${PASS:?set FABRIC_PASS or admin password file}"
ADMIT="${FLUXVM_AI_ADMIT_TOKEN:-}"
if [[ -z "$ADMIT" && -f /etc/systemd/system/zyvor-fabricd.service.d/ai-janus.conf ]]; then
  ADMIT="$(grep '^Environment=FLUXVM_AI_ADMIT_TOKEN=' /etc/systemd/system/zyvor-fabricd.service.d/ai-janus.conf | cut -d= -f3 || true)"
fi

curl_json() { curl -sk "$@"; }

echo "== Janus health =="
curl -sf "$JANUS_URL/api/health" | jq -c .

echo "== fabricd login =="
TOKEN=$(curl_json -X POST "$FABRIC_URL/api/auth/login" \
  -H 'content-type: application/json' \
  -d "{\"username\":\"$USER\",\"password\":\"$PASS\"}" | jq -r .token)
[[ -n "$TOKEN" && "$TOKEN" != null ]]
auth=(-H "authorization: Bearer $TOKEN" -H 'content-type: application/json')

echo "== inference nodes =="
NODES=$(curl_json "${auth[@]}" "$FABRIC_URL/api/ai/nodes")
echo "$NODES" | jq -c '[.[] | {id, site, state, gpus:[.gpus[]? | {bdf, model, vram_gib, parent_bdf}]}]'
echo "$NODES" | jq -e '[.[].gpus[]? | select(.bdf | startswith("janus:"))] | length > 0' >/dev/null

# Free Janus parents held by leftover lab deployments so this smoke can place.
echo "== free janus allocations =="
while read -r dep; do
  [[ -z "$dep" ]] && continue
  echo "releasing $dep"
  for ep in $(curl_json "${auth[@]}" "$FABRIC_URL/api/ai/endpoints" | jq -r --arg d "$dep" '.[]? | select(.deployment==$d) | .name'); do
    curl_json -X DELETE "${auth[@]}" "$FABRIC_URL/api/ai/endpoints/$ep" >/dev/null || true
  done
  curl_json -X DELETE "${auth[@]}" "$FABRIC_URL/api/ai/deployments/$dep" >/dev/null || true
done < <(curl_json "${auth[@]}" "$FABRIC_URL/api/ai/deployments" | jq -r '
  .[]?
  | select(
      (.name | startswith("janus-lab"))
      or any(.status.replicas[]?; (.bdf // "") | startswith("janus:"))
    )
  | .name
')
for _ in $(seq 1 20); do
  held=$(curl_json "${auth[@]}" "$FABRIC_URL/api/ai/deployments" | jq '[.[]? | select(any(.status.replicas[]?; (.bdf // "") | startswith("janus:")))] | length')
  [[ "$held" == "0" ]] && break
  sleep 2
done
held=$(curl_json "${auth[@]}" "$FABRIC_URL/api/ai/deployments" | jq '[.[]? | select(any(.status.replicas[]?; (.bdf // "") | startswith("janus:")))] | length')
[[ "$held" == "0" ]] || { echo "Janus GPU still allocated after release" >&2; exit 1; }

NAME="janus-lab-$(date +%s)"
echo "== ensure model/profile/deploy/endpoint =="
curl_json -X POST "${auth[@]}" "$FABRIC_URL/api/ai/models" \
  -d "{\"name\":\"$NAME-model\",\"source\":\"hf://janus/probe\",\"format\":\"safetensors\"}" >/dev/null || true
curl_json -X POST "${auth[@]}" "$FABRIC_URL/api/ai/profiles" \
  -d "{\"name\":\"$NAME-profile\",\"runtime\":\"vllm\",\"gpu\":{\"vendor\":\"nvidia\",\"count\":1,\"minimum_vram_gib\":0},\"cpu\":2,\"memory_gib\":8}" >/dev/null || true
curl_json -X POST "${auth[@]}" "$FABRIC_URL/api/ai/deployments" \
  -d "{\"name\":\"$NAME\",\"model\":\"$NAME-model\",\"profile\":\"$NAME-profile\",\"replicas\":1}" >/dev/null || true
curl_json -X POST "${auth[@]}" "$FABRIC_URL/api/ai/endpoints" \
  -d "{\"name\":\"$NAME-ep\",\"deployment\":\"$NAME\",\"protocol\":\"openai\",\"port\":8000}" >/dev/null || true

for _ in $(seq 1 24); do
  PHASE=$(curl_json "${auth[@]}" "$FABRIC_URL/api/ai/deployments/$NAME" | jq -r '.status.phase // empty')
  ADDR=$(curl_json "${auth[@]}" "$FABRIC_URL/api/ai/deployments/$NAME" | jq -r '.status.replicas[0].address // empty')
  if [[ "$PHASE" == "Ready" && -n "$ADDR" ]]; then
    break
  fi
  sleep 2
done
DEP=$(curl_json "${auth[@]}" "$FABRIC_URL/api/ai/deployments/$NAME")
echo "$DEP" | jq -c '{phase:.status.phase, address:.status.replicas[0].address, bdf:.status.replicas[0].bdf}'
echo "$DEP" | jq -e '.status.phase == "Ready"' >/dev/null
echo "$DEP" | jq -e '.status.replicas[0].bdf | startswith("janus:")' >/dev/null

echo "== gateway chat =="
KEY_JSON=$(curl_json -X POST "${auth[@]}" "$FABRIC_URL/api/ai/keys" \
  -d "{\"name\":\"$NAME-key\",\"endpoint\":\"$NAME-ep\",\"request_quota\":20}")
SECRET=$(echo "$KEY_JSON" | jq -r .secret)
CHAT=$(curl_json -X POST "$FABRIC_URL/api/ai/openai/$NAME-ep/v1/chat/completions" \
  -H "authorization: Bearer $SECRET" -H 'content-type: application/json' \
  -d "{\"model\":\"$NAME-model\",\"messages\":[{\"role\":\"user\",\"content\":\"hi\"}]}")
echo "$CHAT" | jq -c '{content:.choices[0].message.content}'
echo "$CHAT" | jq -e '.choices[0].message.content | test("Janus")' >/dev/null

NODE_ID=$(echo "$NODES" | jq -r '.[0].id')
PARENT=$(echo "$NODES" | jq -r '.[0].gpus[] | select((.parent_bdf // "") == "") | .bdf' | head -1)
echo "== MIG catalog =="
curl_json -o /tmp/janus-mig-bad.json -w 'bad=%{http_code}\n' -X POST "${auth[@]}" \
  "$FABRIC_URL/api/ai/nodes/$NODE_ID/mig" \
  -d "{\"parent_bdf\":\"$PARENT\",\"profile\":\"9g.bogus\"}"
[[ "$(jq -r .error /tmp/janus-mig-bad.json)" == "unknown MIG profile" ]]

# Free the Janus parent: delete this smoke endpoint/deployment first.
curl_json -X DELETE "${auth[@]}" "$FABRIC_URL/api/ai/endpoints/$NAME-ep" >/dev/null
curl_json -X DELETE "${auth[@]}" "$FABRIC_URL/api/ai/deployments/$NAME" >/dev/null
sleep 8
MIG_CODE=$(curl_json -o /tmp/janus-mig-ok.json -w '%{http_code}' -X POST "${auth[@]}" \
  "$FABRIC_URL/api/ai/nodes/$NODE_ID/mig" \
  -d "{\"parent_bdf\":\"$PARENT\",\"profile\":\"1g.10gb\"}")
echo "ok=$MIG_CODE"
if [[ "$MIG_CODE" == "409" ]]; then
  echo "MIG create skipped (parent still allocated by another deployment)"
elif [[ "$MIG_CODE" == "200" ]]; then
  SLICE=$(jq -r --arg p "$PARENT" '.gpus[]? | select(.parent_bdf == $p) | .bdf' /tmp/janus-mig-ok.json | head -1)
  [[ -n "$SLICE" ]]
  curl_json -o /dev/null -w 'del=%{http_code}\n' -X DELETE "${auth[@]}" \
    "$FABRIC_URL/api/ai/nodes/$NODE_ID/mig/$SLICE"
else
  echo "unexpected MIG status $MIG_CODE" >&2
  head -c 300 /tmp/janus-mig-ok.json >&2 || true
  exit 1
fi

if [[ -n "$ADMIT" ]]; then
  echo "== admit token =="
  curl_json -o /tmp/janus-admit.json -w 'admit=%{http_code}\n' -X POST \
    -H 'content-type: application/json' \
    "$FABRIC_URL/api/ai/admit/$ADMIT" \
    -d "{\"tenant\":\"default\",\"model\":\"$NAME-model\",\"gpus\":1}"
  jq -e '.allowed == true' /tmp/janus-admit.json >/dev/null
fi

# Cleanup leftovers
for id in $(curl_json "${auth[@]}" "$FABRIC_URL/api/ai/keys" | jq -r --arg n "$NAME-key" '.[]? | select(.name==$n) | .id'); do
  curl_json -X DELETE "${auth[@]}" "$FABRIC_URL/api/ai/keys/$id" >/dev/null || true
done
curl_json -X DELETE "${auth[@]}" "$FABRIC_URL/api/ai/profiles/$NAME-profile" >/dev/null || true
curl_json -X DELETE "${auth[@]}" "$FABRIC_URL/api/ai/models/$NAME-model" >/dev/null || true

echo "OK janus lab smoke"
