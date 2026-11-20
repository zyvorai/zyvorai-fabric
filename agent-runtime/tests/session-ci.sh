#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0
# Deploys the real ops agent plus a waiter and a Claude harness, then checks
# sessions, MCP, schedules, webhooks, loops, delegation, and approval.
# The FluxVM stand-in runs the guest on this machine. No provider API is called.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/zyvor-agent-session.XXXXXX")"
BASE="http://127.0.0.1:19096"
STUB_PORT=17788
BIN="${ZYVOR_AGENT_BIN:-$ROOT/agent-runtime/target/debug/zyvor-fabric-agent-runtime}"
CLI="$ROOT/sdk/agent-runtime/src/cli.js"
FAKE_BIN="$WORK/bin"
STUB_PID=""
RUNTIME_PID=""

json_get() {
  python3 -c 'import json,sys
data=json.load(sys.stdin)
cur=data
for part in sys.argv[1].split("."):
    if part == "":
        continue
    cur = cur[int(part)] if part.isdigit() else cur[part]
if cur is None:
    sys.exit(2)
print(cur if not isinstance(cur, (dict, list)) else json.dumps(cur))' "$1"
}

fail() {
  echo "session-ci: $*" >&2
  echo "----- stub -----" >&2
  tail -n 80 "$WORK/stub.log" >&2 || true
  echo "----- runtime -----" >&2
  tail -n 80 "$WORK/runtime.log" >&2 || true
  echo "----- guests -----" >&2
  tail -n 40 "$WORK"/sandboxes/*.log >&2 || true
  exit 1
}

cleanup() {
  if [[ -n "$RUNTIME_PID" ]]; then kill "$RUNTIME_PID" 2>/dev/null || true; fi
  if [[ -n "$STUB_PID" ]]; then kill "$STUB_PID" 2>/dev/null || true; fi
  wait "$RUNTIME_PID" "$STUB_PID" 2>/dev/null || true
}
trap cleanup EXIT

wait_http() {
  local url=$1
  local i
  for i in $(seq 1 100); do
    if curl -sf -o /dev/null "$url"; then
      return 0
    fi
    sleep 0.2
  done
  return 1
}

# Poll GET /v1/sessions/{id} until a terminal status. Failed sessions fail the job.
poll_session() {
  local id=$1
  local deadline=$((SECONDS + 60))
  local body status
  while (( SECONDS < deadline )); do
    body="$(curl -sf "$BASE/v1/sessions/$id")" || fail "session $id disappeared"
    status="$(printf '%s' "$body" | json_get status)"
    case "$status" in
      completed) printf '%s' "$body"; return 0 ;;
      failed|cancelled|expired)
        echo "$body" >&2
        fail "session $id ended $status"
        ;;
    esac
    sleep 0.4
  done
  echo "$body" >&2
  fail "session $id did not finish within 60s"
}

wait_status() {
  local id=$1 want=$2
  local deadline=$((SECONDS + 60))
  local body status
  while (( SECONDS < deadline )); do
    body="$(curl -sf "$BASE/v1/sessions/$id")" || fail "session $id disappeared"
    status="$(printf '%s' "$body" | json_get status)"
    if [[ "$status" == "$want" ]]; then
      return 0
    fi
    case "$status" in
      failed|cancelled|expired)
        echo "$body" >&2
        fail "session $id ended $status while waiting for $want"
        ;;
    esac
    sleep 0.4
  done
  echo "$body" >&2
  fail "session $id did not reach $want within 60s"
}

prepare_workspace() {
  local dir=/opt/zyvor/agent/workspace
  if mkdir -p "$dir" 2>/dev/null && [[ -w "$dir" ]]; then
    return 0
  fi
  if sudo -n mkdir -p "$dir" 2>/dev/null && sudo -n chmod -R a+rwx /opt/zyvor 2>/dev/null; then
    return 0
  fi
  # GitHub-hosted runners have passwordless sudo and use /opt/zyvor, which is
  # the harness default. A developer machine that cannot write there relocates
  # only this process; the guest still receives the path through the environment.
  export ZYVOR_HARNESS_WORKSPACE="$WORK/harness-workspace"
  mkdir -p "$ZYVOR_HARNESS_WORKSPACE"
}

if [[ ! -x "$BIN" ]]; then
  cargo build --manifest-path "$ROOT/agent-runtime/Cargo.toml"
fi
if [[ ! -d "$ROOT/sdk/agent-runtime/node_modules" ]]; then
  npm install --prefix "$ROOT/sdk/agent-runtime" --no-audit --no-fund
fi

prepare_workspace
mkdir -p "$FAKE_BIN" "$WORK/state" "$WORK/snapshots" "$WORK/sandboxes"
cp "$ROOT/agent-runtime/tests/fake-claude.sh" "$FAKE_BIN/claude"
chmod +x "$FAKE_BIN/claude"
export PATH="$FAKE_BIN:$PATH"

SANDBOX_STUB_PORT=$STUB_PORT \
SANDBOX_STUB_ROOT="$WORK/sandboxes" \
  python3 "$ROOT/agent-runtime/tests/sandbox_stub.py" >"$WORK/stub.log" 2>&1 &
STUB_PID=$!
for _ in $(seq 1 50); do
  if curl -s -o /dev/null "http://127.0.0.1:${STUB_PORT}/"; then
    break
  fi
  sleep 0.1
done
curl -s -o /dev/null "http://127.0.0.1:${STUB_PORT}/" || fail "sandbox stub did not start"

# ZYVOR_AGENT_PROXY_LISTEN=off: the CONNECT proxy defaults to 0.0.0.0:18083, the same
# port as the egress broker below, which fails with "Address already in use" on Linux.
# This test does not use the proxy. (Keep comments out of the continued lines.)
ZYVOR_AGENT_ALLOW_NO_AUTH=1 \
ZYVOR_AGENT_LISTEN=127.0.0.1:19096 \
ZYVOR_AGENT_EGRESS_LISTEN=127.0.0.1:18083 \
ZYVOR_AGENT_PROXY_LISTEN=off \
ZYVOR_AGENT_FLUXVM_URL="http://127.0.0.1:${STUB_PORT}" \
ZYVOR_AGENT_EGRESS_ADVERTISE_HOST=127.0.0.1 \
ZYVOR_AGENT_STATE_DIR="$WORK/state" \
ZYVOR_AGENT_SNAPSHOT_DIR="$WORK/snapshots" \
ZYVOR_AGENT_GUEST_START_TIMEOUT_SECS=45 \
RUST_LOG=info \
  "$BIN" >"$WORK/runtime.log" 2>&1 &
RUNTIME_PID=$!
wait_http "$BASE/healthz" || fail "agent runtime did not start"

export FABRIC_AGENT_URL="$BASE"
deploy() {
  node "$CLI" deploy "$@" --template ci
}

deploy "$ROOT/examples/agent-runtime/ops-agent.ts" --name ops-agent --runtime-port 18080
deploy "$ROOT/agent-runtime/tests/waiter.mjs" --name waiter --runtime-port 18081
deploy "$ROOT/agent-runtime/tests/harness-prompt.md" \
  --name harness --runtime claude --runtime-port 18082

echo "session-ci: schedules"
schedule="$(curl -sf -X POST "$BASE/v1/schedules" \
  -H 'content-type: application/json' \
  -d '{"agent":"ops-agent","cron":"0 0 1 1 *","input":{}}')" \
  || fail "schedule was rejected"
schedule_id="$(printf '%s' "$schedule" | json_get id)"
[[ "$(printf '%s' "$schedule" | json_get cron)" == "0 0 1 1 *" ]] || fail "schedule cron was not stored"
curl -sf -X DELETE "$BASE/v1/schedules/$schedule_id" -o /dev/null

echo "session-ci: ops-agent session"
ops="$(curl -sf -X POST "$BASE/v1/sessions" \
  -H 'content-type: application/json' \
  -d '{"agent":"ops-agent","input":{}}')" || fail "ops session was rejected"
ops_id="$(printf '%s' "$ops" | json_get id)"
poll_session "$ops_id" >/dev/null
events="$(curl -sN --max-time 10 "$BASE/v1/sessions/$ops_id/events")" || true
printf '%s' "$events" | grep -q 'healthcheck.completed' \
  || fail "ops session journal missing healthcheck.completed"

echo "session-ci: mcp"
tools="$(curl -sf -X POST "$BASE/mcp" \
  -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}')" || fail "tools/list failed"
printf '%s' "$tools" | grep -q 'list_agents' || fail "tools/list missing list_agents"
printf '%s' "$tools" | grep -q 'list_executions' || fail "tools/list missing list_executions"
printf '%s' "$tools" | grep -q 'chat_with_agent' || fail "tools/list missing chat_with_agent"
chat="$(curl -sf -X POST "$BASE/mcp" \
  -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"chat_with_agent","arguments":{"agent":"waiter","message":"hold"}}}')" \
  || fail "chat_with_agent failed"
waiter_id="$(printf '%s' "$chat" | python3 -c 'import json,sys
body=json.load(sys.stdin)
text=body["result"]["content"][0]["text"]
print(json.loads(text)["session"]["id"])')"
[[ -n "$waiter_id" ]] || fail "chat_with_agent did not return a session id"
wait_status "$waiter_id" running

echo "session-ci: webhooks"
hook="$(curl -sf -X POST "$BASE/v1/webhooks" \
  -H 'content-type: application/json' \
  -d '{"agent":"ops-agent","input":{}}')" || fail "webhook create failed"
hook_id="$(printf '%s' "$hook" | json_get id)"
secret="$(printf '%s' "$hook" | json_get secret)"
body='{"ping":true}'
bad="$(curl -s -o /dev/null -w '%{http_code}' -X POST "$BASE/v1/hooks/$hook_id" \
  -H 'content-type: application/json' \
  -H 'X-Zyvor-Signature: sha256=00' \
  --data "$body")"
[[ "$bad" == "401" ]] || fail "bad webhook signature returned $bad"
sig="$(printf '%s' "$body" | python3 -c 'import hmac,hashlib,sys
secret=sys.argv[1]
print("sha256="+hmac.new(secret.encode(), sys.stdin.buffer.read(), hashlib.sha256).hexdigest())' "$secret")"
admitted="$(curl -sf -X POST "$BASE/v1/hooks/$hook_id" \
  -H 'content-type: application/json' \
  -H "X-Zyvor-Signature: $sig" \
  --data "$body")" || fail "signed webhook was rejected"
hook_session="$(printf '%s' "$admitted" | json_get session_id)"
poll_session "$hook_session" >/dev/null

echo "session-ci: loops"
loop="$(curl -sf -X POST "$BASE/v1/loops" \
  -H 'content-type: application/json' \
  -d '{"agent":"ops-agent","input":{},"bounds":{"max_runs":1}}')" || fail "loop create failed"
loop_id="$(printf '%s' "$loop" | json_get id)"
loop_session=""
deadline=$((SECONDS + 60))
while (( SECONDS < deadline )); do
  listed="$(curl -sf "$BASE/v1/loops")" || fail "loop list failed"
  loop_session="$(printf '%s' "$listed" | python3 -c 'import json,sys
want=sys.argv[1]
for item in json.load(sys.stdin)["items"]:
    if item["id"]==want and item.get("stopped_reason")=="max_runs" and item.get("active_session_id"):
        print(item["active_session_id"])
        raise SystemExit
sys.exit(2)' "$loop_id" || true)"
  if [[ -n "$loop_session" ]]; then
    break
  fi
  sleep 1
done
[[ -n "$loop_session" ]] || fail "loop did not admit one session and stop"
poll_session "$loop_session" >/dev/null

echo "session-ci: delegation"
child="$(curl -sf -X POST "$BASE/v1/sessions/$waiter_id/delegate" \
  -H 'content-type: application/json' \
  -d '{"agent":"ops-agent","input":{}}')" || fail "delegate failed"
child_id="$(printf '%s' "$child" | json_get id)"
child_agent="$(printf '%s' "$child" | json_get agent)"
child_parent="$(printf '%s' "$child" | json_get parent_session_id)"
[[ "$child_agent" == "ops-agent" ]] || fail "delegated child used $child_agent"
[[ "$child_parent" == "$waiter_id" ]] || fail "child parent_session_id was $child_parent"
poll_session "$child_id" >/dev/null
child_events="$(curl -sN --max-time 10 "$BASE/v1/sessions/$child_id/events")" || true
printf '%s' "$child_events" | grep -q 'healthcheck.completed' \
  || fail "delegated child did not run the ops-agent manifest"

echo "session-ci: harness approval"
harness="$(curl -sf -X POST "$BASE/v1/sessions" \
  -H 'content-type: application/json' \
  -d '{"agent":"harness","input":"review the change"}')" || fail "harness session was rejected"
harness_id="$(printf '%s' "$harness" | json_get id)"
approval_id=""
deadline=$((SECONDS + 60))
while (( SECONDS < deadline )); do
  approvals="$(curl -sf "$BASE/v1/approvals")" || fail "approval list failed"
  approval_id="$(printf '%s' "$approvals" | python3 -c 'import json,sys
want=sys.argv[1]
for item in json.load(sys.stdin)["items"]:
    if item["session_id"]==want and item["status"]=="pending" and "ship it" in item["prompt"]:
        print(item["id"])
        raise SystemExit
sys.exit(2)' "$harness_id" || true)"
  if [[ -n "$approval_id" ]]; then
    break
  fi
  body="$(curl -sf "$BASE/v1/sessions/$harness_id")" || fail "harness session disappeared"
  status="$(printf '%s' "$body" | json_get status)"
  case "$status" in
    failed|cancelled|expired) echo "$body" >&2; fail "harness session ended $status before approval" ;;
  esac
  sleep 0.4
done
[[ -n "$approval_id" ]] || fail "harness did not open an approval"
curl -sf -X POST "$BASE/v1/approvals/$approval_id" \
  -H 'content-type: application/json' \
  -d '{"decision":"approved"}' >/dev/null || fail "approval decision was rejected"
poll_session "$harness_id" >/dev/null

echo "session-ci: hello-go"
command -v go >/dev/null || fail "go is not on PATH"
deploy "$ROOT/examples/agent-runtime/hello-go-agent.ts" --name hello-go --runtime-port 18084
hello="$(curl -sf -X POST "$BASE/v1/sessions" \
  -H 'content-type: application/json' \
  -d '{"agent":"hello-go","input":{"task":"print hello"}}')" || fail "hello-go session was rejected"
hello_id="$(printf '%s' "$hello" | json_get id)"
poll_session "$hello_id" >/dev/null
hello_events="$(curl -sN --max-time 10 "$BASE/v1/sessions/$hello_id/events")" || true
printf '%s\n' "$hello_events" | python3 -c 'import json,sys
found=None
for block in sys.stdin.read().split("\n\n"):
    data=None
    for line in block.splitlines():
        if line.startswith("data:"):
            data=line[5:].strip()
    if not data:
        continue
    event=json.loads(data)
    if event.get("kind")=="session.result":
        found=event["data"]
if not found:
    sys.exit("session.result missing")
source=found.get("source","")
stdout=found.get("stdout","")
if "package main" not in source or "fmt.Println(\"hello\")" not in source:
    sys.exit("hello-go did not return its Go source")
if stdout.strip()!="hello":
    sys.exit("hello-go stdout was %r" % stdout)
print(json.dumps(found, indent=2))' || fail "hello-go did not return source and hello"

echo "session-ci: ok"
