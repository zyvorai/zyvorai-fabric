#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: Apache-2.0
#
# Deploy one Keep packaged agent and exercise goals → artifact → approval.
#
#   ./scripts/merion-pack-demo.sh [infra-ops|migration-op|deploy-op] [--live-session]
#
# Env:
#   MERION_API / MERION_TOKEN   agent-runtime (default http://127.0.0.1:9096)
#   FABRIC_HOST             hostname for credential/policy (default 127.0.0.1)
#   FABRIC_API_BASE         fabricd origin for session input (default https://FABRIC_HOST:9095)
#   SKIP_DEPLOY=1           only create goal/artifact (agent already deployed)
#   MERION_PACK_DRY=1         skip fabric-agent build if unavailable; stub bundle
#   MERION_E2E_TEMPLATE       template for --live-session (default agent-node)
#
# Pre-rename names still work: KEEP_<X> is used when MERION_<X> is unset.
[ -n "${MERION_API+x}" ] || [ -z "${KEEP_API:-}" ] || export MERION_API="$KEEP_API"
[ -n "${MERION_E2E_TEMPLATE+x}" ] || [ -z "${KEEP_E2E_TEMPLATE:-}" ] || export MERION_E2E_TEMPLATE="$KEEP_E2E_TEMPLATE"
[ -n "${MERION_PACK_DRY+x}" ] || [ -z "${KEEP_PACK_DRY:-}" ] || export MERION_PACK_DRY="$KEEP_PACK_DRY"
[ -n "${MERION_POLICY_SEED+x}" ] || [ -z "${KEEP_POLICY_SEED:-}" ] || export MERION_POLICY_SEED="$KEEP_POLICY_SEED"
[ -n "${MERION_TOKEN+x}" ] || [ -z "${KEEP_TOKEN:-}" ] || export MERION_TOKEN="$KEEP_TOKEN"

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PACK="infra-ops"
LIVE_SESSION=0
for arg in "$@"; do
  case "$arg" in
    --live-session) LIVE_SESSION=1 ;;
    infra-ops|migration-op|deploy-op) PACK=$arg ;;
    -h|--help) sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown arg: $arg" >&2; exit 1 ;;
  esac
done
PACK_DIR="$ROOT/examples/merion-agents/$PACK"
KEEPCTL="$ROOT/scripts/merionctl"

if [[ ! -d "$PACK_DIR" ]]; then
  echo "unknown pack: $PACK (expected infra-ops|migration-op|deploy-op)" >&2
  exit 1
fi

BASE="${MERION_API:-${ZYVOR_AGENT_URL:-http://127.0.0.1:9096}}"
TOKEN="${MERION_TOKEN:-${ZYVOR_AGENT_TOKEN:-}}"
FABRIC_HOST="${FABRIC_HOST:-127.0.0.1}"
FABRIC_API_BASE="${FABRIC_API_BASE:-https://${FABRIC_HOST}:9095}"
TEMPLATE="${MERION_E2E_TEMPLATE:-agent-node}"

auth=()
if [[ -n "$TOKEN" ]]; then
  auth=(-H "Authorization: Bearer $TOKEN")
fi

api() {
  local method=$1 path=$2; shift 2
  curl -fsS -X "$method" "${auth[@]}" "$BASE$path" "$@"
}

echo "==> pack=$PACK keep=$BASE fabric_host=$FABRIC_HOST live_session=$LIVE_SESSION"

if [[ "${SKIP_DEPLOY:-}" != "1" ]]; then
  WORK="$(mktemp -d "${TMPDIR:-/tmp}/keep-pack.XXXXXX")"
  trap 'rm -rf "$WORK"' EXIT

  BUNDLE_FILE="$WORK/bundle.mjs"
  if command -v npx >/dev/null 2>&1 && [[ -f "$ROOT/sdk/agent-runtime/package.json" ]]; then
    echo "==> fabric-agent build"
    (cd "$ROOT/sdk/agent-runtime" && npm ci --silent >/dev/null 2>&1 || npm ci)
    (cd "$ROOT/sdk/agent-runtime" && npx fabric-agent build "$PACK_DIR/agent.ts" --out "$BUNDLE_FILE")
  elif [[ "${MERION_PACK_DRY:-}" == "1" ]]; then
    echo "==> MERION_PACK_DRY=1 — stub bundle (no Fabric calls)"
    printf 'export default async function(ctx){ ctx.emit("pack.stub",{pack:"%s"}); return {ok:true,stub:true}; }\n' "$PACK" >"$BUNDLE_FILE"
  else
    echo "fabric-agent / npx not available; set MERION_PACK_DRY=1 for stub deploy" >&2
    exit 1
  fi

  BUNDLE_B64=$(base64 <"$BUNDLE_FILE" | tr -d '\n')
  DEPLOY="$WORK/deploy.json"
  # Omit fabric-api grant when the host vault is not configured (goals/artifacts still work).
  INCLUDE_CREDS=0
  if [[ -n "${FABRIC_API_TOKEN:-}" ]] || [[ -n "${ZYVOR_AGENT_CREDENTIALS_FILE:-}" ]]; then
    INCLUDE_CREDS=1
  fi
  python3 - "$PACK_DIR/deploy.json" "$BUNDLE_B64" "$FABRIC_HOST" "$DEPLOY" "$INCLUDE_CREDS" <<'PY'
import json, sys
src, b64, host, out, include_creds = sys.argv[1:6]
doc = json.load(open(src))
doc["bundle_base64"] = b64
doc["name"] = doc.get("name") or "pack"
m = doc.setdefault("manifest", {})
m["egress_allow_hosts"] = [host]
m["template"] = m.get("template") or "agent-node"
# Prefer agent-node for labs without node22-agent registered
if m.get("template") == "node22-agent":
    m["template"] = "agent-node"
if "taint" in m and isinstance(m["taint"], dict):
    m["taint"]["trusted_hosts"] = [host]
if include_creds != "1":
    m.pop("credentials", None)
json.dump(doc, open(out, "w"), indent=2)
print(out)
PY

  # Prefer real template when live
  if [[ "$LIVE_SESSION" == "1" ]]; then
    python3 - "$DEPLOY" "$TEMPLATE" <<'PY'
import json,sys
p, tmpl = sys.argv[1:3]
d=json.load(open(p)); d["manifest"]["template"]=tmpl
json.dump(d, open(p,"w"), indent=2)
PY
  fi

  echo "==> deploy $PACK"
  if [[ -n "${MERION_POLICY_SEED:-}" ]]; then
    "$KEEPCTL" policy sign "$DEPLOY" >/dev/null
  elif [[ ! -f "${DEPLOY}.sig" ]]; then
    echo "note: Keep mode needs a signed deploy — set MERION_POLICY_SEED or provide ${DEPLOY}.sig" >&2
  fi
  RESP=$("$KEEPCTL" create -f "$DEPLOY" 2>&1) || {
    echo "$RESP" >&2
    echo "deploy failed — is agent-runtime up at $BASE?" >&2
    echo "hint: merge examples/merion-agents/_fabric/credentials.fabric-api.json into ZYVOR_AGENT_CREDENTIALS_FILE" >&2
    echo "hint: Keep mode needs merionctl policy sign + X-Merion-Manifest-Signature" >&2
    exit 1
  }
  echo "$RESP"
fi

SESSION_ID=""
if [[ "$LIVE_SESSION" == "1" ]]; then
  echo "==> create live session (template=$TEMPLATE fabricBase=$FABRIC_API_BASE)"
  SESS=$(api POST /v1/sessions -H 'Content-Type: application/json' -d "$(python3 -c "
import json
print(json.dumps({
  'agent': '$PACK',
  'user_id': 'pack-demo',
  'input': {'fabricBase': '$FABRIC_API_BASE'},
}))
")")
  SESSION_ID=$(printf '%s' "$SESS" | python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])')
  echo "    session_id=$SESSION_ID"
  # Wait briefly for guest work / events
  for _ in $(seq 1 60); do
    st=$(api GET "/v1/sessions/$SESSION_ID" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("status",""))' 2>/dev/null || true)
    case "$st" in running|completed|failed|cancelled|expired) echo "    status=$st"; break ;; esac
    sleep 2
  done
fi

echo "==> create goal"
GOAL_JSON=$(api POST /v1/goals -H 'Content-Type: application/json' -d "$(python3 - "$PACK" "$SESSION_ID" <<'PY'
import json, sys
pack = sys.argv[1]
sid = sys.argv[2] or None
plans = {
  "infra-ops": [
    {"title": "Read alerts and VM inventory", "requires_approval": False},
    {"title": "Apply remediation / restart", "requires_approval": True},
  ],
  "migration-op": [
    {"title": "Readiness + inspect", "requires_approval": False},
    {"title": "Create or cancel migration", "requires_approval": True},
  ],
  "deploy-op": [
    {"title": "Probe readyz/health", "requires_approval": False},
    {"title": "Operator runs install commands", "requires_approval": False},
  ],
}
body = {
  "title": f"{pack} demo goal",
  "description": "merion-pack-demo.sh",
  "agent": pack,
  "plan": plans.get(pack, [{"title": "Run", "requires_approval": False}]),
}
if sid:
  body["session_id"] = sid
print(json.dumps(body))
PY
)")
GOAL_ID=$(printf '%s' "$GOAL_JSON" | python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])')
echo "    goal_id=$GOAL_ID"

echo "==> create artifact"
ART_BODY="# ${PACK} diagnosis

Generated by merion-pack-demo.sh against fabricd.
"
if [[ -n "$SESSION_ID" ]]; then
  ART_BODY="${ART_BODY}
Session: ${SESSION_ID}
fabricBase: ${FABRIC_API_BASE}
"
fi
ART_JSON=$(api POST /v1/artifacts -H 'Content-Type: application/json' -d "$(python3 - "$PACK" "$GOAL_ID" "$SESSION_ID" "$ART_BODY" <<'PY'
import json, sys
pack, gid, sid, body = sys.argv[1:5]
print(json.dumps({
  "kind": "incident-timeline" if pack == "infra-ops" else "demo-report",
  "title": f"{pack} diagnosis",
  "body": body,
  "goal_id": gid,
  "session_id": sid or None,
  "agent": pack,
}))
PY
)")
ART_ID=$(printf '%s' "$ART_JSON" | python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])')
echo "    artifact_id=$ART_ID"

echo "==> advance read step"
api POST "/v1/goals/${GOAL_ID}/advance" -H 'Content-Type: application/json' \
  -d "{\"step_id\":\"s1\",\"status\":\"done\",\"artifact_id\":\"$ART_ID\"}" >/dev/null

if [[ "$PACK" != "deploy-op" ]]; then
  if [[ -n "$SESSION_ID" ]]; then
    echo "==> advance approval step (opens /v1/approvals)"
    ADV=$(api POST "/v1/goals/${GOAL_ID}/advance" -H 'Content-Type: application/json' \
      -d '{"step_id":"s2","status":"done","approval_prompt":"Approve infra-ops proposed fix?"}' || true)
    echo "$ADV" | python3 -c 'import json,sys; g=json.load(sys.stdin); print("goal", g["status"], "approval", g["plan"][1].get("approval_id"))' 2>/dev/null || echo "$ADV"
    echo "    Keep view: /app/merion/$SESSION_ID"
  else
    echo "==> note: approval step needs a session_id on the goal"
    echo "    PATCH /v1/goals/$GOAL_ID {\"session_id\":\"…\"} then"
    echo "    POST /v1/goals/$GOAL_ID/advance {\"step_id\":\"s2\",\"status\":\"done\"}"
    echo "    → opens /v1/approvals; decide then mutating Fabric calls proceed"
  fi
fi

echo "==> goal"
api GET "/v1/goals/${GOAL_ID}" | python3 -m json.tool
echo
echo "PASS keep-pack-demo $PACK (goal=$GOAL_ID artifact=$ART_ID${SESSION_ID:+ session=$SESSION_ID})"
echo "Pack README: examples/merion-agents/$PACK/README.md"
