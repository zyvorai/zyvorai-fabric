#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

# Lab smoke for Fabric AI Workloads Phases 3–6 (dry-run).
# Usage: FABRIC_URL=https://127.0.0.1:8443 ./scripts/smoke-ai-workloads-phases.sh
set -euo pipefail

FABRIC_URL="${FABRIC_URL:-https://127.0.0.1:8443}"
USER="${FABRIC_USER:-admin}"
PASS="${FABRIC_PASS:-}"
if [[ -z "$PASS" && -f /var/lib/zyvor-fabricd/.admin_password ]]; then
  PASS="$(cat /var/lib/zyvor-fabricd/.admin_password)"
fi
PASS="${PASS:?set FABRIC_PASS or admin password file}"

curl_json() {
  curl -sk "$@"
}

echo "== login =="
TOKEN=$(curl_json -X POST "$FABRIC_URL/api/auth/login" \
  -H 'content-type: application/json' \
  -d "{\"username\":\"$USER\",\"password\":\"$PASS\"}" | jq -r .token)
[[ -n "$TOKEN" && "$TOKEN" != null ]]

auth=(-H "authorization: Bearer $TOKEN" -H 'content-type: application/json')
NAME="smoke-phases-$(date +%s)"

echo "== model =="
curl_json -X POST "$FABRIC_URL/api/ai/models" "${auth[@]}" \
  -d "{\"name\":\"$NAME-model\",\"source\":\"hf://Qwen/Qwen3-8B\",\"format\":\"safetensors\"}" | jq -c '{name,source}'

echo "== profile =="
curl_json -X POST "$FABRIC_URL/api/ai/profiles" "${auth[@]}" \
  -d "{\"name\":\"$NAME-profile\",\"runtime\":\"vllm\",\"gpu\":{\"vendor\":\"nvidia\",\"count\":1,\"minimum_vram_gib\":24},\"cpu\":4,\"memory_gib\":16}" | jq -c '{name}'

echo "== deploy =="
curl_json -X POST "$FABRIC_URL/api/ai/deployments" "${auth[@]}" \
  -d "{\"name\":\"$NAME\",\"model\":\"$NAME-model\",\"profile\":\"$NAME-profile\",\"replicas\":2,\"preferred_site\":\"lab\",\"autoscaling\":{\"enabled\":true,\"min_replicas\":1,\"max_replicas\":3,\"scale_out_queue\":5,\"scale_out_seconds\":1,\"scale_in_queue\":1,\"scale_in_seconds\":3600}}" | jq -c '{name,replicas,autoscaling}'

sleep 3
echo "== status =="
PHASE1=$(curl_json "$FABRIC_URL/api/ai/deployments/$NAME" "${auth[@]}" | jq -r .status.phase)
echo "phase=$PHASE1"
REV1=$(curl_json "$FABRIC_URL/api/ai/deployments/$NAME/revisions" "${auth[@]}" | jq -r '.[0].revision')
echo "revision=$REV1"
if [[ "$REV1" != "1" ]]; then
  echo "expected revision 1, got $REV1"
  exit 1
fi
if [[ -n "${FABRICD_BIN:-}" && -n "${FABRICD_PIDFILE:-}" ]]; then
  restart_fabricd() {
    echo "== restart fabricd =="
    old="$(cat "$FABRICD_PIDFILE")"
    kill "$old" || true
    for _ in $(seq 1 50); do
      if ! kill -0 "$old" 2>/dev/null; then
        break
      fi
      sleep 0.2
    done
    "$FABRICD_BIN" >> "${FABRICD_LOG:-/tmp/fabricd-smoke.log}" 2>&1 &
    echo $! > "$FABRICD_PIDFILE"
    for _ in $(seq 1 90); do
      if curl -skf "$FABRIC_URL/health" >/dev/null; then
        break
      fi
      sleep 1
    done
    curl -skf "$FABRIC_URL/health" >/dev/null
  }
  IDS1=$(curl_json "$FABRIC_URL/api/ai/deployments/$NAME" "${auth[@]}" | jq -c '[.status.replicas[] | {replica_id,ordinal,vm_name}]')
  for n in 1 2; do
    restart_fabricd
    PHASE=$(curl_json "$FABRIC_URL/api/ai/deployments/$NAME" "${auth[@]}" | jq -r .status.phase)
    IDS=$(curl_json "$FABRIC_URL/api/ai/deployments/$NAME" "${auth[@]}" | jq -c '[.status.replicas[] | {replica_id,ordinal,vm_name}]')
    if [[ -z "$PHASE1" || "$PHASE1" == "null" || "$PHASE1" != "$PHASE" ]]; then
      echo "phase changed across restart $n: $PHASE1 -> $PHASE" >&2
      exit 1
    fi
    if [[ "$IDS" != "$IDS1" ]]; then
      echo "replica identity changed across restart $n: $IDS1 -> $IDS" >&2
      exit 1
    fi
    echo "restart $n kept phase $PHASE and replicas $IDS"
  done
else
  echo "== reconcile again =="
  sleep 16
  PHASE2=$(curl_json "$FABRIC_URL/api/ai/deployments/$NAME" "${auth[@]}" | jq -r .status.phase)
  if [[ -z "$PHASE1" || "$PHASE1" == "null" || "$PHASE1" != "$PHASE2" ]]; then
    echo "phase changed across reconcile: $PHASE1 -> $PHASE2" >&2
    exit 1
  fi
  echo "phase stayed $PHASE2"
fi

echo "== endpoint =="
curl_json -X POST "$FABRIC_URL/api/ai/endpoints" "${auth[@]}" \
  -d "{\"name\":\"$NAME-ep\",\"deployment\":\"$NAME\",\"protocol\":\"openai\",\"port\":8000,\"routing_strategy\":\"site_local\",\"preferred_site\":\"lab\"}" | jq -c '{name,routing_strategy,preferred_site}'

sleep 12
echo "== metrics =="
curl_json "$FABRIC_URL/api/ai/deployments/$NAME/metrics" "${auth[@]}" | jq -c '{phase,weights:[.replicas[].maglev_weight]}'

echo "== drain =="
curl_json -X POST "$FABRIC_URL/api/ai/deployments/$NAME/drain" "${auth[@]}" \
  -d '{"grace_seconds":5}' | jq -c '{phase:.status.phase,draining:[.status.replicas[].draining]}'

echo "== api key =="
KEY_JSON=$(curl_json -X POST "$FABRIC_URL/api/ai/keys" "${auth[@]}" \
  -d "{\"name\":\"$NAME-key\",\"endpoint\":\"$NAME-ep\"}")
echo "$KEY_JSON" | jq -c '{id:.key.id,prefix:.key.prefix,secret:(.secret|.[0:12])}'
SECRET=$(echo "$KEY_JSON" | jq -r .secret)
echo "== sse =="
SSE=$(curl_json -N -X POST "$FABRIC_URL/api/ai/openai/$NAME-ep/v1/chat/completions" \
  -H "authorization: Bearer $SECRET" \
  -H 'content-type: application/json' \
  -d "{\"model\":\"$NAME-model\",\"stream\":true,\"messages\":[{\"role\":\"user\",\"content\":\"hi\"}]}")
DATA_LINE=$(printf '%s\n' "$SSE" | grep -n 'data:' | head -1 | cut -d: -f1 || true)
DONE_LINE=$(printf '%s\n' "$SSE" | grep -n '\[DONE\]' | head -1 | cut -d: -f1 || true)
if [[ -z "$DATA_LINE" || -z "$DONE_LINE" || "$DATA_LINE" -ge "$DONE_LINE" ]]; then
  echo "expected two SSE chunks, got:" >&2
  printf '%s\n' "$SSE" >&2
  exit 1
fi
echo "sse chunks ok"

echo "== cleanup =="
curl_json -X DELETE "$FABRIC_URL/api/ai/endpoints/$NAME-ep" "${auth[@]}" -o /dev/null -w '%{http_code}\n'
# delete keys
for id in $(curl_json "$FABRIC_URL/api/ai/keys" "${auth[@]}" | jq -r --arg n "$NAME-key" '.[]|select(.name==$n)|.id'); do
  curl_json -X DELETE "$FABRIC_URL/api/ai/keys/$id" "${auth[@]}" -o /dev/null
done
curl_json -X DELETE "$FABRIC_URL/api/ai/deployments/$NAME" "${auth[@]}" -o /dev/null -w '%{http_code}\n'
curl_json -X DELETE "$FABRIC_URL/api/ai/profiles/$NAME-profile" "${auth[@]}" -o /dev/null
curl_json -X DELETE "$FABRIC_URL/api/ai/models/$NAME-model" "${auth[@]}" -o /dev/null
echo "OK phases smoke"
