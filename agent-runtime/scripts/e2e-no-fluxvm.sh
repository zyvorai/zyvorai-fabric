#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

# End-to-end check of the agent runtime binary on real sockets. No FluxVM needed:
# sessions are seeded on disk and the reviewer is a local mock. Needs Linux-style
# bash 4 (associative arrays), python3, curl, and outbound HTTPS to example.com/.org.
#   cargo build --release && agent-runtime/scripts/e2e-no-fluxvm.sh
set -uo pipefail
export PATH="$HOME/.cargo/bin:$PATH"
BIN="${BIN:-$(cd "$(dirname "$0")/.." && pwd)/target/release/zyvor-fabric-agent-runtime}"
W="$(mktemp -d /tmp/zyvor-agent-e2e.XXXXXX)"
API=http://127.0.0.1:19096
PROXY=127.0.0.1:19083
TOKEN=e2e-token
PASS=0; FAIL=0
pids=()
cleanup() { for p in "${pids[@]}"; do kill "$p" 2>/dev/null; done; rm -rf "$W"; }
trap cleanup EXIT

check() { # name, expected, actual
  if [[ "$3" == *"$2"* ]]; then echo "PASS  $1"; PASS=$((PASS+1)); else echo "FAIL  $1  (want '$2', got '$3')"; FAIL=$((FAIL+1)); fi
}
check_eq() { # name, expected, actual (exact match; an empty expected value means "nothing")
  if [[ "$3" == "$2" ]]; then echo "PASS  $1"; PASS=$((PASS+1)); else echo "FAIL  $1  (want exactly '$2', got '$3')"; FAIL=$((FAIL+1)); fi
}
api() { curl -s -o /dev/stderr -w '%{http_code}' -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' "$@" 2>"$W/body"; }
body() { cat "$W/body"; }

# --- mock reviewer: deny anything with "evil" in the host, escalate the rest
cat > "$W/reviewer.py" <<'PY'
import json, http.server
class H(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        n = int(self.headers['Content-Length']); req = json.loads(self.rfile.read(n))
        sent = json.loads(req['messages'][1]['content'])
        v = 'deny' if 'evil' in sent['host'] else 'escalate'
        out = json.dumps({'choices':[{'message':{'content':json.dumps({'verdict':v,'reason':'mock saw '+sent['host']})}}]}).encode()
        self.send_response(200); self.send_header('Content-Type','application/json'); self.send_header('Content-Length',str(len(out))); self.end_headers(); self.wfile.write(out)
    def log_message(self,*a): pass
http.server.HTTPServer(('127.0.0.1',19100),H).serve_forever()
PY
python3 "$W/reviewer.py" & pids+=($!)

# --- aux servers: a signed-webhook receiver (19101) and a counting upstream (19102)
cat > "$W/aux.py" <<'PY'
import json, sys, threading, http.server
W = sys.argv[1]
class Hook(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        n = int(self.headers['Content-Length']); body = self.rfile.read(n).decode()
        with open(f"{W}/hooks.jsonl", "a") as f:
            f.write(json.dumps({"sig": self.headers.get("x-zyvor-signature"), "body": body}) + "\n")
        self.send_response(200); self.send_header('Content-Length','0'); self.end_headers()
    def log_message(self,*a): pass
class Upstream(http.server.BaseHTTPRequestHandler):
    def handle_any(self):
        n = int(self.headers.get('Content-Length') or 0); body = self.rfile.read(n).decode()
        with open(f"{W}/upstream.log", "a") as f:
            f.write(json.dumps({"method": self.command, "path": self.path, "body": body}) + "\n")
        out = b"ok"; self.send_response(200); self.send_header('Content-Length', str(len(out))); self.end_headers(); self.wfile.write(out)
    do_GET = do_POST = handle_any
    def log_message(self,*a): pass
threading.Thread(target=http.server.HTTPServer(('127.0.0.1',19101),Hook).serve_forever, daemon=True).start()
http.server.HTTPServer(('127.0.0.1',19102),Upstream).serve_forever()
PY
python3 "$W/aux.py" "$W" & pids+=($!)
cat > "$W/creds.json" <<'JSON'
{"mail": {"host": "127.0.0.1", "header": "authorization", "kind": "fabric",
          "allowed_ports": [19102], "requires_approval": ["POST"], "approval_kind": "send"}}
JSON

start_runtime() {
  ZYVOR_AGENT_API_TOKEN=$TOKEN ZYVOR_AGENT_LISTEN=127.0.0.1:19096 ZYVOR_AGENT_EGRESS_LISTEN=127.0.0.1:19082 \
  ZYVOR_AGENT_PROXY_LISTEN=$PROXY ZYVOR_AGENT_STATE_DIR="$W/state" ZYVOR_AGENT_SNAPSHOT_DIR="$W/snap" \
  ZYVOR_AGENT_FLUXVM_URL=http://127.0.0.1:1 ZYVOR_AGENT_SYNC_INTERVAL_MS=3600000 \
  ZYVOR_AGENT_MAX_VCPUS=2 ZYVOR_AGENT_MAX_MEMORY_MIB=8192 \
  ZYVOR_AGENT_SENTINEL_URL=http://127.0.0.1:19100/v1 ZYVOR_AGENT_SENTINEL_MODEL=mock \
  ZYVOR_AGENT_APPROVAL_WEBHOOK=http://127.0.0.1:19101/hook ZYVOR_AGENT_APPROVAL_WEBHOOK_SECRET=hook-secret \
  ZYVOR_AGENT_CREDENTIALS_FILE="$W/creds.json" \
  "$BIN" >"$W/runtime.log" 2>&1 & RT=$!; pids+=($RT)
  for _ in $(seq 50); do curl -s -o /dev/null "$API/v1/agents" -H "Authorization: Bearer $TOKEN" && return 0; sleep 0.2; done
  echo "runtime did not start"; cat "$W/runtime.log"; exit 1
}
BUNDLE=$(printf 'export default 1' | base64)
deploy() { # name, manifest-json
  api -X POST "$API/v1/agents" -d "{\"name\":\"$1\",\"bundle_base64\":\"$BUNDLE\",\"manifest\":$2}"
}

start_runtime
echo "== deploy validation"
check "resources over the vcpu ceiling are rejected" 400 "$(deploy sized '{"template":"t","resources":{"vcpus":4,"memory_mib":4096}}')"
check "  ...with a clear reason" "exceeds this runtime's limit" "$(body)"
check "resources within the ceilings deploy" 200 "$(deploy sized2 '{"template":"t","resources":{"vcpus":2,"memory_mib":7900}}' | sed 's/^20[01]$/200/')"
check "shared home volume still needs max_concurrent_sessions 1" 400 "$(deploy shared '{"template":"t","home_volume":{}}')"
check "per-user home volume drops that rule" 200 "$(deploy peruser '{"template":"t","home_volume":{"per_user":true}}' | sed 's/^20[01]$/200/')"
check "browser template manifest (per-user + sized + sentinel)" 200 "$(deploy browser '{"template":"browser-agent","resources":{"vcpus":2,"memory_mib":7900},"home_volume":{"per_user":true},"egress_mode":"sentinel","egress_allow_hosts":["example.com"]}' | sed 's/^20[01]$/200/')"

echo "== session user_id"
check "per-user agent requires user_id" 400 "$(api -X POST "$API/v1/sessions" -d '{"agent":"peruser"}')"
check "  ...with a clear reason" "user_id is required" "$(body)"
check "malformed user_id is rejected" 400 "$(api -X POST "$API/v1/sessions" -d '{"agent":"peruser","user_id":"Bad User"}')"
code=$(api -X POST "$API/v1/sessions" -d '{"agent":"peruser","user_id":"alice"}')
check "valid user_id passes validation (then fails only at FluxVM: 502)" 502 "$code"

# --- deploy the proxy agents, then seed sessions for them
deploy allow '{"template":"t","egress_allow_hosts":["example.com"]}' >/dev/null
deploy denyall '{"template":"t"}' >/dev/null
deploy sentinel '{"template":"t","egress_mode":"sentinel","egress_approval_timeout_seconds":60}' >/dev/null
deploy mailer '{"template":"t","credentials":["mail"],"egress_allow_hosts":["127.0.0.1"],"allow_private_networks":true,"dlp":true,"taint":{"trusted_hosts":[]},"egress_approval_timeout_seconds":60,"egress_rules":[{"host":"127.0.0.1","methods":["GET","POST"],"max_body_bytes":4096}]}' >/dev/null
declare -A VER CAP SID
for a in allow denyall sentinel mailer; do
  VER[$a]=$(curl -s -H "Authorization: Bearer $TOKEN" "$API/v1/agents/$a" | python3 -c 'import sys,json;print(json.load(sys.stdin)["version"])')
done
kill $RT; wait $RT 2>/dev/null; pids=("${pids[@]/$RT}")
for a in allow denyall sentinel mailer; do
  SID[$a]=$(python3 -c 'import uuid;print(uuid.uuid4())'); CAP[$a]=cap-$a-$RANDOM
  mkdir -p "$W/state/sessions/${SID[$a]}"
  python3 - "$a" "${SID[$a]}" "${VER[$a]}" "${CAP[$a]}" "$W" <<'PY'
import sys, json, uuid, datetime
a, sid, ver, cap, w = sys.argv[1:]
now = datetime.datetime.now(datetime.timezone.utc).isoformat()
json.dump({"id": sid, "agent": a, "agent_version": ver, "sandbox_id": str(uuid.uuid4()), "status": "running",
           "input": {}, "created_at": now, "updated_at": now, "last_event_seq": 0, "guest_event_cursor": 0,
           "capability_token": cap}, open(f"{w}/state/sessions/{sid}/session.json", "w"))
PY
done
start_runtime

px() { # agent, host:port, extra curl args -> http code of the CONNECT+request
  curl -s -o /dev/null -w '%{http_code}' --max-time 30 -x "http://${SID[$1]}:${CAP[$1]}@$PROXY" "https://$2/" "${@:3}"
}
raw() { # agent, connect target -> first response line (raw CONNECT)
  python3 - "$PROXY" "${SID[$1]}" "${CAP[$1]}" "$2" <<'PY'
import sys, socket, base64
hp, sid, cap, target = sys.argv[1:]
h, p = hp.split(":"); s = socket.create_connection((h, int(p)), 10)
auth = base64.b64encode(f"{sid}:{cap}".encode()).decode()
s.sendall(f"CONNECT {target} HTTP/1.1\r\nHost: {target}\r\nProxy-Authorization: Basic {auth}\r\n\r\n".encode())
print(s.recv(600).decode().replace("\r\n", " | "))
PY
}

echo "== CONNECT proxy (real HTTPS through the runtime)"
check "allowlisted host: real TLS tunnel to example.com returns 200" 200 "$(px allow example.com)"
check "unlisted host in deny mode is refused" "403" "$(raw denyall example.com:443)"
check "non-443 port is refused" "403" "$(raw allow example.com:25)"
check "private destination is blocked even for an allowlisted-style target" "403" "$(raw denyall 127.0.0.1:443)"
check "bad capability gets 407" 407 "$(curl -s -o /dev/null -w '%{http_connect}' -x "http://${SID[allow]}:wrong@$PROXY" https://example.com/)"
check "plain http:// through the proxy is refused" 405 "$(curl -s -o /dev/null -w '%{http_code}' --max-time 10 -x "http://${SID[allow]}:${CAP[allow]}@$PROXY" http://example.com/)"

echo "== Sentinel"
check "reviewer deny refuses with the reviewer's reason" "denied by sentinel" "$(raw sentinel evil.example:443)"
# escalate: request is held until an operator approves it through the API
( px sentinel example.org > "$W/held.code" ) & held=$!
for _ in $(seq 60); do
  aid=$(curl -s -H "Authorization: Bearer $TOKEN" "$API/v1/approvals" | python3 -c 'import sys,json
d=json.load(sys.stdin); it=d["items"] if isinstance(d,dict) else d
p=[a for a in it if a["status"]=="pending"]; print(p[0]["id"] if p else "")')
  [[ -n "$aid" ]] && break; sleep 0.5
done
check "escalated request opens an egress approval carrying the reviewer note" "mock saw example.org" \
  "$(curl -s -H "Authorization: Bearer $TOKEN" "$API/v1/approvals")"
check "operator approves once" 200 "$(api -X POST "$API/v1/approvals/$aid" -d '{"decision":"approved","scope":"once"}')"
wait $held
check "held tunnel then completes with a real 200" 200 "$(cat "$W/held.code")"


echo "== approvals reach a human, out of band"
check "approval webhook was delivered" approval.requested "$(cat "$W/hooks.jsonl" 2>/dev/null)"
sig_ok=$(python3 - "$W/hooks.jsonl" <<'PY'
import sys, json, hmac, hashlib
line = json.loads(open(sys.argv[1]).readline())
want = "sha256=" + hmac.new(b"hook-secret", line["body"].encode(), hashlib.sha256).hexdigest()
print("valid" if hmac.compare_digest(want, line["sig"]) else "INVALID")
PY
)
check "  ...and its HMAC signature verifies" valid "$sig_ok"
check "  ...and it says which kind, and how to decide" '/v1/approvals/' "$(head -1 "$W/hooks.jsonl")"
check "the agent's capability cannot list approvals" 401 "$(curl -s -o /dev/null -w '%{http_code}' -H "Authorization: Bearer ${CAP[mailer]}" "$API/v1/approvals")"
check "the agent's capability cannot decide approvals" 401 "$(curl -s -o /dev/null -w '%{http_code}' -X POST -H "Authorization: Bearer ${CAP[mailer]}" -H 'Content-Type: application/json' -d '{"decision":"approved"}' "$API/v1/approvals/$(python3 -c 'import uuid;print(uuid.uuid4())')")"
check "the egress broker port serves no approval route" 404 "$(curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:19082/v1/approvals)"

broker() { # agent, method, body, credential -> http code (JSON body in $W/broker.out)
  local payload
  payload=$(python3 - "$2" "$3" "$4" <<'PY'
import sys, json, base64
method, body, cred = sys.argv[1:]
req = {"url": "http://127.0.0.1:19102/send?token=hunter2", "method": method}
if body: req["body_base64"] = base64.b64encode(body.encode()).decode()
if cred: req["credential"] = cred
print(json.dumps(req))
PY
)
  curl -s -o "$W/broker.out" -w '%{http_code}' --max-time 90 -X POST http://127.0.0.1:19082/v1/egress \
    -H "x-zyvor-session-id: ${SID[$1]}" -H "x-zyvor-egress-capability: ${CAP[$1]}" -H 'Content-Type: application/json' -d "$payload"
}
upstream_count() { [[ -f "$W/upstream.log" ]] && wc -l < "$W/upstream.log" | tr -d ' ' || echo 0; }
pending_json() { curl -s -H "Authorization: Bearer $TOKEN" "$API/v1/approvals" | python3 -c 'import sys,json
d=json.load(sys.stdin); it=d["items"] if isinstance(d,dict) else d
p=[a for a in it if a["status"]=="pending"]; print(json.dumps(p[0]) if p else "")'; }
wait_pending_json() { local j; for _ in $(seq 60); do j=$(pending_json); [[ -n "$j" ]] && { echo "$j"; return; }; sleep 0.5; done; }
decide() { api -X POST "$API/v1/approvals/$1" -d "{\"decision\":\"$2\",\"scope\":\"once\"}"; }

echo "== send approvals, DLP and rules through the broker"
before=$(upstream_count)
( broker mailer POST "hello secret body" mail > "$W/send.code" ) & sending=$!
pj=$(wait_pending_json)
check "a credential that requires approval holds the POST" '"kind": "send"' "$(python3 -c 'import sys,json;print(json.dumps(json.loads(sys.argv[1]),indent=1))' "$pj")"
check "  ...showing a body digest" body_sha256 "$pj"
if [[ "$pj" == *"hello secret body"* || "$pj" == *hunter2* ]]; then echo "FAIL  approval leaked the body or query"; FAIL=$((FAIL+1)); else echo "PASS  ...but never the body or the query string"; PASS=$((PASS+1)); fi
check_eq "  ...and nothing reached the upstream while it was pending" "$before" "$(upstream_count)"
aid=$(python3 -c 'import sys,json;print(json.loads(sys.argv[1])["id"])' "$pj")
check "operator approves the send" 200 "$(decide "$aid" approved)"
wait $sending
check "the held send then completes" 200 "$(cat "$W/send.code")"
check_eq "  ...and reached the upstream once" "$((before+1))" "$(upstream_count)"

# That POST's reply came from an untrusted host, so the session is now tainted.
check "reading from an untrusted host taints the session" 127.0.0.1 "$(curl -s -H "Authorization: Bearer $TOKEN" "$API/v1/sessions/${SID[mailer]}")"
before=$(upstream_count)
( broker mailer POST "second" "" > "$W/tainted.code" ) & sending=$!
pj=$(wait_pending_json)
check "a write from a tainted session is held even without a credential" "tainted by 127.0.0.1" "$pj"
decide "$(python3 -c 'import sys,json;print(json.loads(sys.argv[1])["id"])' "$pj")" denied >/dev/null
wait $sending
check "  ...and a denial stops it" 403 "$(cat "$W/tainted.code")"
check_eq "  ...with nothing sent" "$before" "$(upstream_count)"
check "reads from a tainted session still flow" 200 "$(broker mailer GET "" "")"
check "the agent cannot clear its own taint" 401 "$(curl -s -o /dev/null -w '%{http_code}' -X POST -H "Authorization: Bearer ${CAP[mailer]}" "$API/v1/sessions/${SID[mailer]}/untaint")"
check "the operator clears the taint" 200 "$(api -X POST "$API/v1/sessions/${SID[mailer]}/untaint")"
check "  ...and it is gone" '"tainted_by":[]' "$(curl -s -H "Authorization: Bearer $TOKEN" "$API/v1/sessions/${SID[mailer]}" | tr -d ' ')"

before=$(upstream_count)
( broker mailer POST "aws=AKIAIOSFODNN7EXAMPLE" "" > "$W/dlp.code" ) & sending=$!
pj=$(wait_pending_json)
check "DLP holds a request carrying a key" aws-access-key "$pj"
[[ "$pj" == *AKIAIOSFODNN7EXAMPLE* ]] && { echo "FAIL  approval leaked the secret"; FAIL=$((FAIL+1)); } || { echo "PASS  ...and the approval never contains the secret"; PASS=$((PASS+1)); }
decide "$(python3 -c 'import sys,json;print(json.loads(sys.argv[1])["id"])' "$pj")" denied >/dev/null
wait $sending
check "  ...and a denial stops it" 403 "$(cat "$W/dlp.code")"
check "an oversized body is refused by the host's egress rules, before any approval" 403 "$(broker mailer POST "$(python3 -c 'print("x"*5000)')" "")"
check "  ...naming the rule" "egress rules" "$(cat "$W/broker.out")"
check_eq "no approval was opened for it" "" "$(pending_json)"

echo "== workstations, confidential, browser, inner container"
check "confidential + warm pool is rejected at deploy" 400 "$(deploy badconf '{"template":"t","confidential":"auto","warm_pool_size":1}')"
check "confidential: required + home volume is rejected" 400 "$(deploy badconf2 '{"template":"t","confidential":"required","home_volume":{"per_user":true}}')"
check "a persistent, contained, browser agent deploys" 200 "$(deploy desk '{"template":"t","persistent":true,"inner_container":"strict","confidential":"auto","browser_port":9222,"home_volume":{"per_user":true}}' | sed 's/^20[01]$/200/')"
check "a non-persistent agent cannot have a workstation" 409 "$(api -X PUT "$API/v1/workstations/allow/alice")"
check "the agent's capability cannot create workstations" 401 "$(curl -s -o /dev/null -w '%{http_code}' -X PUT -H "Authorization: Bearer ${CAP[allow]}" "$API/v1/workstations/desk/alice")"
check "the operator declares a workstation" 201 "$(api -X PUT "$API/v1/workstations/desk/alice")"
for _ in $(seq 20); do
  ws=$(curl -s -H "Authorization: Bearer $TOKEN" "$API/v1/workstations/desk/alice")
  [[ "$ws" == *'"consecutive_failures":0'* ]] || break; sleep 0.5
done
check "the loop tried to start it and recorded the failure (FluxVM is unreachable here)" '"last_error"' "$ws"
check_eq "  ...with a backoff scheduled" True "$(python3 -c 'import sys,json;print(json.loads(sys.argv[1]).get("next_attempt_at") is not None)' "$ws")"
check "the tab view refuses an agent without browser_port" 404 "$(curl -s -o /dev/null -w '%{http_code}' -H "Authorization: Bearer $TOKEN" "$API/v1/sessions/${SID[allow]}/browser/json/list")"
check "  ...and mutating DevTools paths" 404 "$(curl -s -o /dev/null -w '%{http_code}' -H "Authorization: Bearer $TOKEN" "$API/v1/sessions/${SID[allow]}/browser/json/new")"
check "the operator removes the workstation" 204 "$(api -X DELETE "$API/v1/workstations/desk/alice")"

echo "== journal"
audit=$(curl -s -H "Authorization: Bearer $TOKEN" "$API/v1/audit?limit=200")
check "egress.connect entries recorded" egress.connect "$audit"
check "sentinel.egress denial recorded" sentinel.egress "$audit"
check "tunnel byte counts recorded" bytes_down "$audit"
verify=$(curl -s -H "Authorization: Bearer $TOKEN" "$API/v1/audit/verify" 2>/dev/null); echo "audit verify: ${verify:0:120}"

echo; echo "passed=$PASS failed=$FAIL"
[[ $FAIL -eq 0 ]]
