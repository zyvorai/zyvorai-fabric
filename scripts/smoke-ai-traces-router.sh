#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

# End-to-end smoke for trace capture, the decision router and A/B reports.
# Needs no GPU: two mock OpenAI backends stand in for model arms.
#   FABRIC_URL=http://127.0.0.1:19095 FABRIC_PASS=... ./scripts/smoke-ai-traces-router.sh
# Requires a running fabricd, python3 and curl. Creates ai-smoke-* resources and
# removes them on exit (KEEP=1 leaves them, and the mock backends, running).
set -euo pipefail

FABRIC_URL="${FABRIC_URL:-http://127.0.0.1:9095}"
FABRIC_USER="${FABRIC_USER:-admin}"
FABRIC_PASS="${FABRIC_PASS:?set FABRIC_PASS}"
HERE="$(cd "$(dirname "$0")" && pwd)"
PORT_BIG="${PORT_BIG:-18001}" PORT_SMALL="${PORT_SMALL:-18002}"
N="${N:-120}"
CURL=(curl -sk -m 30)

pids=()
reset() { # remove ai-smoke-* resources, whether left by an earlier run or made by this one
  api DELETE "/api/ai/routers/ai-smoke-front" >/dev/null 2>&1 || true
  api DELETE "/api/ai/trace-policies/ai-smoke-front" >/dev/null 2>&1 || true
  api DELETE "/api/ai/traces?endpoint=ai-smoke-front" >/dev/null 2>&1 || true
  for id in $(api GET /api/ai/keys 2>/dev/null | python3 -c 'import sys,json; print(" ".join(k["id"] for k in json.load(sys.stdin) if k["name"]=="ai-smoke-key"))' 2>/dev/null); do
    api DELETE "/api/ai/keys/$id" >/dev/null 2>&1 || true
  done
  for s in front big small judge; do
    api DELETE "/api/ai/endpoints/ai-smoke-$s" >/dev/null 2>&1 || true
    api DELETE "/api/ai/deployments/ai-smoke-$s" >/dev/null 2>&1 || true
  done
}
cleanup() {
  [[ "${KEEP:-}" == 1 ]] && return 0
  for p in "${pids[@]:-}"; do [[ -n "$p" ]] && kill "$p" 2>/dev/null || true; done
  reset
}
trap cleanup EXIT

jget() { python3 -c "import sys,json; d=json.load(sys.stdin); print($1)"; }
fail() { echo "FAIL: $*" >&2; exit 1; }
ok() { echo "ok   $*"; }

TOKEN=$("${CURL[@]}" -X POST "$FABRIC_URL/api/auth/login" -H 'content-type: application/json' \
  -d "{\"username\":\"$FABRIC_USER\",\"password\":\"$FABRIC_PASS\"}" | jget 'd["token"]')
api() { local m=$1 p=$2; shift 2; "${CURL[@]}" -X "$m" "$FABRIC_URL$p" -H "authorization: Bearer $TOKEN" -H 'content-type: application/json' "$@"; }

reset
python3 "$HERE/mock-openai.py" "$PORT_BIG" big 0 & pids+=($!)
python3 "$HERE/mock-openai.py" "$PORT_SMALL" small 40 & pids+=($!)
sleep 1

api POST /api/ai/models -d '{"name":"ai-smoke-model","source":"hf://x/y","format":"safetensors"}' >/dev/null || true
api POST /api/ai/profiles -d '{"name":"ai-smoke-profile","runtime":"vllm","gpu":{"vendor":"nvidia","count":1,"minimum_vram_gib":0},"cpu":2,"memory_gib":8}' >/dev/null || true
# The front door is backed by the same mock as "big"; it is only the entry point.
for pair in "big:$PORT_BIG" "small:$PORT_SMALL" "front:$PORT_BIG" "judge:$PORT_BIG"; do
  s=${pair%%:*}; port=${pair##*:}
  api POST /api/ai/deployments -d "{\"name\":\"ai-smoke-$s\",\"model\":\"ai-smoke-model\",\"profile\":\"ai-smoke-profile\",\"replicas\":1}" >/dev/null
  api POST /api/ai/endpoints -d "{\"name\":\"ai-smoke-$s\",\"deployment\":\"ai-smoke-$s\",\"protocol\":\"openai\",\"port\":$port,\"vip\":\"127.0.0.1\"}" >/dev/null
done
KEY=$(api POST /api/ai/keys -d '{"name":"ai-smoke-key","endpoint":"ai-smoke-front"}')
SECRET=$(echo "$KEY" | jget 'd.get("secret", "")')
KEYID=$(echo "$KEY" | jget 'd.get("key", {}).get("id", "")')
[[ -n "$SECRET" ]] || fail "no API key secret in: $KEY"
GW="$FABRIC_URL/api/ai/openai/ai-smoke-front/v1/chat/completions"
chat() { # user prompt [extra-json-fields] -> headers, blank line, body
  "${CURL[@]}" -i -X POST "$GW" -H "authorization: Bearer $SECRET" -H 'content-type: application/json' \
    -H "x-user-id: $1" -d "{\"model\":\"m\",\"messages\":[{\"role\":\"user\",\"content\":\"$2\"}]${3:+,$3}}"
}


hdr() { echo "$1" | tr -d '\r' | grep -i "^$2:" | head -1 | sed 's/^[^:]*: *//'; }
set_router() { api POST /api/ai/routers -d "$1" >/dev/null || fail "router rejected: $1"; }

echo "== 1. no policy: plain proxy, nothing recorded =="
out=$(chat u0 hello)
echo "$out" | head -1 | grep -q 200 || fail "plain request: $out"
[[ -z "$(hdr "$out" x-fabric-trace-id)" ]] || fail "trace id without a policy"
ok "plain proxy"

echo "== 2. trace capture with token IDs and auto-score =="
api POST /api/ai/trace-policies -d '{"endpoint":"ai-smoke-front","capture_token_ids":true,"auto_score":{"kind":"finish_stop"}}' >/dev/null
out=$(chat u1 "capture me")
TID=$(hdr "$out" x-fabric-trace-id); [[ -n "$TID" ]] || fail "no x-fabric-trace-id: $out"
sleep 0.3
tr=$(api GET "/api/ai/traces/$TID")
echo "$tr" | jget 'd["completion"].startswith("big:") and d["completion_token_ids"] and d["prompt_token_ids"]==[1,2,3,4] and len(d["logprobs"])==len(d["completion_token_ids"]) and d["score"]==1.0 and d["status"]==200 and d["request"]["messages"][0]["content"]=="capture me"' | grep -q True || fail "trace content: $tr"
ok "non-stream trace has completion, token IDs, logprobs, score"
out=$(chat u1 "stream me" '"stream":true')
STID=$(hdr "$out" x-fabric-trace-id); sleep 0.3
api GET "/api/ai/traces/$STID" | jget 'd["streamed"] and d["completion"].strip().startswith("big: reply to stream me") and d["completion_tokens"]>0' | grep -q True || fail "stream trace"
ok "streaming trace reassembled from SSE (usage requested via stream_options)"

echo "== 3. redaction =="
api POST /api/ai/trace-policies -d '{"endpoint":"ai-smoke-front","redact":true}' >/dev/null
out=$(chat u1 "secret prompt"); RID=$(hdr "$out" x-fabric-trace-id); sleep 0.3
api GET "/api/ai/traces/$RID" | jget '"request" not in d and "completion" not in d and len(d["request_sha256"])==64' | grep -q True || fail "redaction"
ok "redacted trace keeps hash and counts only"

echo "== 4. 50/50 split router, sticky per user =="
api POST /api/ai/trace-policies -d '{"endpoint":"ai-smoke-front","auto_score":{"kind":"finish_stop"}}' >/dev/null
set_router '{"endpoint":"ai-smoke-front","strategy":"split","control":"big","candidates":[{"name":"big","endpoint":"ai-smoke-big","weight":1,"quality":0.9,"cost_per_mtok_in":5,"cost_per_mtok_out":15},{"name":"small","endpoint":"ai-smoke-small","weight":1,"quality":0.6,"cost_per_mtok_in":0.2,"cost_per_mtok_out":0.6}]}'
nb=0; ns=0
for i in $(seq 1 "$N"); do
  v=$(hdr "$(chat "user$i" "question number $i")" x-fabric-variant)
  case "$v" in big) nb=$((nb+1));; small) ns=$((ns+1));; *) fail "no variant header ($v)";; esac
done
echo "     big=$nb small=$ns"
(( nb > N*30/100 && ns > N*30/100 )) || fail "split is lopsided: big=$nb small=$ns"
ok "traffic split across both arms"
first=$(hdr "$(chat sticky-user "hi")" x-fabric-variant)
for i in 1 2 3 4 5 6; do [[ "$(hdr "$(chat sticky-user "again $i")" x-fabric-variant)" == "$first" ]] || fail "sticky-user changed arm"; done
ok "same user stays on '$first'"

echo "== 5. A/B report =="
sleep 0.5
rep=$(api GET "/api/ai/routers/ai-smoke-front/report?min_samples=20")
echo "$rep" | python3 -c '
import sys,json
d=json.load(sys.stdin)
for v in d["variants"]:
    print("     %-6s n=%-4d errors=%d mean=%s p95=%dms cost=$%.6f" % (v["variant"],v["requests"],v["errors"],None if v["mean_score"] is None else round(v["mean_score"],3),v["latency_p95_ms"],v["est_cost_usd"]))
print("     ->", d["recommendation"])'
echo "$rep" | jget '[v for v in d["variants"] if v["variant"]=="small"][0]["mean_score"] < [v for v in d["variants"] if v["variant"]=="big"][0]["mean_score"]' | grep -q True || fail "small should score lower"
echo "$rep" | jget '"keep '"'"'big'"'"'" in d["recommendation"]' | grep -q True || fail "expected recommendation to keep big: $rep"
ok "report ranks the arms and recommends keeping the control"

echo "== 6. feedback, filters, export =="
api POST "/api/ai/traces/$TID/feedback" -d '{"score":0.25,"labels":["needs-review"]}' | jget 'd["score"]==0.25 and d["score_source"].startswith("feedback:")' | grep -q True || fail "feedback"
n=$(api GET "/api/ai/traces?router=ai-smoke-front&variant=small&limit=1000" | jget 'len(d)'); (( n > 0 )) || fail "variant filter"
sft=$(api GET "/api/ai/traces/export?router=ai-smoke-front&min_score=1&format=sft&limit=5")
echo "$sft" | head -1 | jget 'd["messages"][-1]["role"]=="assistant"' | grep -q True || fail "sft export: $sft"
ok "feedback stored; variant filter and SFT export work"
api POST "/api/ai/traces/$TID/feedback" -d '{"score":2}' -w '%{http_code}' | grep -q 400 || fail "out-of-range score accepted"
ok "bad score rejected"

echo "== 7. cost router =="
set_router '{"endpoint":"ai-smoke-front","strategy":"cost","min_quality":0.5,"hard_quality":0.8,"candidates":[{"name":"big","endpoint":"ai-smoke-big","quality":0.9,"cost_per_mtok_in":5,"cost_per_mtok_out":15},{"name":"small","endpoint":"ai-smoke-small","quality":0.6,"cost_per_mtok_in":0.2,"cost_per_mtok_out":0.6}]}'
[[ "$(hdr "$(chat c1 "short question")" x-fabric-variant)" == small ]] || fail "cheap arm expected for easy request"
hard=$("${CURL[@]}" -i -X POST "$GW" -H "authorization: Bearer $SECRET" -H 'content-type: application/json' \
  -d '{"model":"m","messages":[{"role":"user","content":"call a tool"}],"tools":[{"type":"function","function":{"name":"f"}}]}')
[[ "$(hdr "$hard" x-fabric-variant)" == big ]] || fail "quality arm expected for tool request"
api POST /api/ai/routers/ai-smoke-front/decide -d '{"messages":[{"role":"user","content":"hi"}]}' | jget 'd["chosen"]=="small"' | grep -q True || fail "decide endpoint"
ok "cost router sends easy work to 'small' and tool use to 'big'"

echo "== 8. fail over when the chosen arm is down =="
kill "${pids[1]}" 2>/dev/null; sleep 0.3
resp=$(chat c2 "short question")
echo "     (arm down) $(echo "$resp" | head -1)"
echo "$resp" | head -1 | grep -q 200 || fail "expected failover to a healthy arm: $resp"
[[ "$(hdr "$resp" x-fabric-variant)" == big ]] || fail "failover should be served by big"
[[ "$(hdr "$resp" x-fabric-failover-from)" == small ]] || fail "missing x-fabric-failover-from"
ok "dead 'small' fails over to 'big' and the response says so"
set_router '{"endpoint":"ai-smoke-front","strategy":"cost","failover":false,"min_quality":0.5,"candidates":[{"name":"big","endpoint":"ai-smoke-big","quality":0.9,"cost_per_mtok_in":5,"cost_per_mtok_out":15},{"name":"small","endpoint":"ai-smoke-small","quality":0.6,"cost_per_mtok_in":0.2,"cost_per_mtok_out":0.6}]}'
chat c3 "short question" | head -1 | grep -q 200 && fail "failover:false must surface the upstream error"
ok "failover:false surfaces the error"
python3 "$HERE/mock-openai.py" "$PORT_SMALL" small 40 & pids[1]=$!; sleep 1

echo "== 9. promote =="
api POST /api/ai/routers/ai-smoke-front/promote -d '{"variant":"big"}' | jget 'd["strategy"]=="split"' | grep -q True || fail "promote"
for i in 1 2 3 4 5 6 7 8; do [[ "$(hdr "$(chat "p$i" hi)" x-fabric-variant)" == big ]] || fail "promote did not pin traffic"; done
ok "all traffic on 'big' after promote"

echo "== 10. LLM judge =="
api POST /api/ai/trace-policies -d '{"endpoint":"ai-smoke-front","judge":{"endpoint":"ai-smoke-judge"}}' >/dev/null
r=$(chat j1 "judge me")
JID=$(hdr "$r" x-fabric-trace-id); [[ -n "$JID" ]] || fail "no trace id: $r"
sleep 2
api GET "/api/ai/traces/$JID" | jget 'd["score"]==0.9 and d["score_source"]=="judge" and d["judge_reason"]=="mock"' | grep -q True || fail "inline judge: $(api GET /api/ai/traces/$JID)"
ok "judge scores a fresh trace in the background (fenced JSON reply parsed)"
api POST "/api/ai/traces/$JID/feedback" -d '{"score":0.1}' >/dev/null
api POST /api/ai/traces/judge -d '{"endpoint":"ai-smoke-front","rejudge":true,"limit":200}' | jget 'd["failed"]==0' | grep -q True || fail "batch judge failed"
api GET "/api/ai/traces/$JID" | jget 'd["score"]==0.1 and d["score_source"].startswith("feedback:")' | grep -q True || fail "judge overwrote human feedback"
ok "human feedback is never overwritten"
api POST /api/ai/trace-policies -d '{"endpoint":"ai-smoke-front"}' >/dev/null
for i in 1 2 3; do chat "jb$i" "batch $i" >/dev/null; done
res=$(api POST /api/ai/traces/judge -d '{"endpoint":"ai-smoke-front","judge":{"endpoint":"ai-smoke-judge"},"limit":200}')
echo "$res" | jget 'd["judged"]>=3 and d["failed"]==0' | grep -q True || fail "batch judge: $res"
ok "batch judge scored unscored traces: $res"
api POST /api/ai/traces/judge -d '{"endpoint":"ai-smoke-front","judge":{"endpoint":"nope"}}' -w '%{http_code}' | grep -q 404 || fail "missing judge endpoint accepted"
ok "missing judge endpoint rejected"

echo "== 11. bandit router learns from scores =="
api POST /api/ai/trace-policies -d '{"endpoint":"ai-smoke-front","auto_score":{"kind":"finish_stop"}}' >/dev/null
api DELETE "/api/ai/traces?endpoint=ai-smoke-front" >/dev/null 2>&1 || true
set_router '{"endpoint":"ai-smoke-front","strategy":"bandit","explore":0.1,"warmup":5,"control":"big","candidates":[{"name":"big","endpoint":"ai-smoke-big","quality":0.9},{"name":"small","endpoint":"ai-smoke-small","quality":0.6}]}'
api POST /api/ai/routers/ai-smoke-front/decide -d '{"messages":[{"role":"user","content":"hi"}]}' | jget '"warm-up" in d["reason"]' | grep -q True || fail "bandit should start in warm-up"
for round in 1 2 3; do
  for i in $(seq 1 25); do chat "b$round-$i" "question $round $i" >/dev/null; done
  sleep 11   # the router refreshes its score cache every 10s
done
big=0; small=0
for i in $(seq 1 60); do
  v=$(hdr "$(chat "bz$i" "late question $i")" x-fabric-variant)
  [[ $v == big ]] && big=$((big+1)) || small=$((small+1))
done
echo "     after learning: big=$big small=$small"
(( big > small * 2 )) || fail "bandit did not shift traffic to the better arm (big=$big small=$small)"
ok "bandit moved most traffic to the higher-scoring arm"
api GET "/api/ai/routers/ai-smoke-front/report?min_samples=10" | jget 'all("p_sequential" in v["vs_control"] for v in d["variants"] if "vs_control" in v)' | grep -q True || fail "report lacks sequential p-value"
ok "report carries the always-valid sequential p-value"

if [[ -n "${FABRICCTL:-}" ]]; then
  echo "== 12. fabricctl =="
  export ZYVOR_FABRIC_URL="$FABRIC_URL/api" ZYVOR_FABRIC_TOKEN="$TOKEN"
  "$FABRICCTL" ai trace policies >/dev/null && "$FABRICCTL" ai trace list --endpoint ai-smoke-front --limit 2 >/dev/null \
    && "$FABRICCTL" ai router report ai-smoke-front --min-samples 20 >/dev/null || fail "fabricctl"
  "$FABRICCTL" ai trace export --router ai-smoke-front --variant small --format sft --limit 3 | head -1 | grep -q messages || fail "fabricctl export"
  ok "fabricctl trace and router commands"
fi
echo "PASS"
