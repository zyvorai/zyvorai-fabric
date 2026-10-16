#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: Apache-2.0
# ============================================================================
# keep-demo-google — try the Gmail and Calendar agents on YOUR Google account, on this laptop. NOT SEALED.
# ============================================================================
#   GOOGLE_CLIENT_ID=... GOOGLE_CLIENT_SECRET=... ./scripts/merion-demo-google.sh
#   ./scripts/merion-demo-google.sh --client-json ~/Downloads/client_secret_....json    # the file Google lets you download
#   ./scripts/merion-demo-google.sh --dry-run     # check the tools and print the plan
#
# You need a Google OAuth client and a test account: docs/merion/connectors/GOOGLE_DEMO.md walks through it (about 10 minutes, in your own
# browser). The client id and secret stay in THIS shell's environment and in this script's runtime; nobody else is given them.
#
# What it does: starts the local simulator and a Keep runtime (like merion-demo-local.sh), then
#   1. asks Google for your consent in your browser (scripts/merion-google-auth.py) unless google-refresh-token.env already exists here;
#   2. gives the token to the runtime as YOUR connection (write-only; never printed);
#   3. deploys gmail-triage, mail-compose and calendar-agent;
#   4. makes a stand-in phone key on this machine and enrols it (a real phone would hold it in its Secure Enclave);
#   5. starts chat pages for the three agents, and a terminal approver where you approve or deny each draft, send or event, looking at
#      the recipients, subject and text the HOST read out of the request.
# Sending mail and creating events stay behind those approvals. Reading mail is not gated, so it reads your real inbox headers.
#
# THIS IS A SIMULATOR for the cell (no VM, no network policy) and a software key for the phone. Everything else (the credential vault, the
# host-side token minting, the approval preview and signature) is the real code.
# ============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BUILD=0
if [[ -n "${KEEP_RUNTIME_BIN:-}" ]]; then BIN="$KEEP_RUNTIME_BIN"; else BIN="$ROOT/agent-runtime/target/debug/zyvor-fabric-agent-runtime"; BUILD=1; fi
SIM_PORT="${KEEP_SIM_PORT:-17788}"
API_PORT="${KEEP_LOCAL_PORT:-19096}"
EGRESS_PORT="${KEEP_LOCAL_EGRESS_PORT:-18082}"
CHAT_PORT="${KEEP_CHAT_PORT:-8787}"
TOKEN_FILE="${KEEP_GOOGLE_TOKEN_FILE:-google-refresh-token.env}"
DRY=0
CLIENT_JSON=""
while (( $# )); do
  case "$1" in
    --dry-run) DRY=1 ;;
    --client-json) shift; CLIENT_JSON="${1:-}"; [[ -n "$CLIENT_JSON" ]] || { echo "--client-json needs a file" >&2; exit 64; } ;;
    -h|--help) sed -n '5,26p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown option: $1" >&2; exit 64 ;;
  esac
  shift
done
# The JSON Google offers to download ({"installed": {"client_id": ..., "client_secret": ...}}). It is read here, in this process, and the values go
# only into this script's environment and the runtime it starts; they are never printed.
if [[ -n "$CLIENT_JSON" ]]; then
  [[ -r "$CLIENT_JSON" ]] || { echo "cannot read $CLIENT_JSON" >&2; exit 1; }
  CREDS="$(python3 - "$CLIENT_JSON" 2>/dev/null <<'PY'
import json, sys
c = json.load(open(sys.argv[1]))
c = c.get("installed") or c.get("web") or {}
print(c["client_id"]); print(c["client_secret"])
PY
  )" || { echo "$CLIENT_JSON is not a Google OAuth client JSON (expected an \"installed\" object with client_id and client_secret)" >&2; exit 1; }
  GOOGLE_CLIENT_ID="$(printf '%s\n' "$CREDS" | sed -n 1p)"; GOOGLE_CLIENT_SECRET="$(printf '%s\n' "$CREDS" | sed -n 2p)"; CREDS=""
  export GOOGLE_CLIENT_ID GOOGLE_CLIENT_SECRET
fi

MISSING=0
need() { command -v "$1" >/dev/null 2>&1 || { echo "missing: $1 ($2)" >&2; MISSING=1; }; }
need python3 "install Python 3"; need curl "install curl"; need node "install Node 20 or newer: https://nodejs.org"
if (( BUILD )) || [[ ! -x "$BIN" ]]; then need cargo "install Rust from https://rustup.rs, or set KEEP_RUNTIME_BIN to a built runtime"; fi
[[ -n "${GOOGLE_CLIENT_ID:-}" ]] || { echo "set GOOGLE_CLIENT_ID (see docs/merion/connectors/GOOGLE_DEMO.md)" >&2; MISSING=1; }
[[ -n "${GOOGLE_CLIENT_SECRET:-}" ]] || { echo "set GOOGLE_CLIENT_SECRET (see docs/merion/connectors/GOOGLE_DEMO.md)" >&2; MISSING=1; }
(( MISSING == 0 )) || exit 1
if (( $(node -p 'process.versions.node.split(".")[0]') < 20 )); then echo "node 20 or newer is required" >&2; exit 1; fi
for p in "$SIM_PORT" "$API_PORT" "$EGRESS_PORT" "$CHAT_PORT" "$((CHAT_PORT+1))" "$((CHAT_PORT+2))"; do
  if (echo >"/dev/tcp/127.0.0.1/$p") 2>/dev/null; then echo "port $p is already in use (see KEEP_SIM_PORT, KEEP_LOCAL_PORT, KEEP_LOCAL_EGRESS_PORT, KEEP_CHAT_PORT)" >&2; exit 1; fi
done

echo "keep-demo-google: SIMULATED cell, software phone key. Real Google account access. Runtime at http://127.0.0.1:$API_PORT"
if (( DRY )); then
  echo "would: build the runtime if needed; get your Google consent in the browser unless $TOKEN_FILE exists; start the simulator and the runtime with the"
  echo "       per-person Google credentials and your client id; connect your account for user 'demo'; deploy gmail-triage, mail-compose and calendar-agent;"
  echo "       make and enrol a stand-in phone key; start chat pages on :$CHAT_PORT (mail), :$((CHAT_PORT+1)) (compose), :$((CHAT_PORT+2)) (calendar);"
  echo "       run the terminal approver until Ctrl-C, then delete the temporary state (the token file in this directory is left for you to delete)"
  exit 0
fi

# consent first, so a refusal or a typo costs nothing else
if [[ ! -s "$TOKEN_FILE" ]]; then
  echo "No $TOKEN_FILE here: opening Google's consent page (drafts, sending and events are asked for; each stays behind an approval)."
  python3 "$SCRIPT_DIR/merion-google-auth.py" --with-drafts --with-send --with-events --out "$TOKEN_FILE"
fi
[[ -s "$TOKEN_FILE" ]] || { echo "no refresh token was written" >&2; exit 1; }

WORK="$(mktemp -d)"
PIDS=()
cleanup() { for p in ${PIDS[@]+"${PIDS[@]}"}; do kill "$p" 2>/dev/null || true; done; wait 2>/dev/null || true; rm -rf "$WORK"; echo; echo "keep-demo-google: stopped, temporary state removed. Revoke access any time at https://myaccount.google.com/permissions"; }
trap cleanup EXIT INT TERM

if (( BUILD )); then echo "building the runtime (incremental)..."; cargo build --manifest-path "$ROOT/agent-runtime/Cargo.toml" 2>&1 | tail -2; fi
[[ -x "$BIN" ]] || { echo "no runtime at $BIN" >&2; exit 1; }
[[ -d "$ROOT/sdk/agent-runtime/node_modules" ]] || npm install --prefix "$ROOT/sdk/agent-runtime" --no-audit --no-fund >/dev/null

OP_TOKEN="$(python3 -c 'import secrets; print(secrets.token_hex(32))')"
mkdir -p "$WORK/sandboxes" "$WORK/state" "$WORK/snap"
# the shipped per-person descriptors as they are (real Google hosts); the client id and secret come from this shell's environment
cp "$ROOT/docs/merion/connectors/google.per-person.credentials.json" "$WORK/creds.json"
SANDBOX_STUB_PORT="$SIM_PORT" SANDBOX_STUB_ROOT="$WORK/sandboxes" python3 "$ROOT/agent-runtime/tests/sandbox_stub.py" >"$WORK/sim.log" 2>&1 &
PIDS+=($!)
env ZYVOR_AGENT_API_TOKEN="$OP_TOKEN" ZYVOR_AGENT_LISTEN="127.0.0.1:$API_PORT" ZYVOR_AGENT_EGRESS_LISTEN="127.0.0.1:$EGRESS_PORT" \
    ZYVOR_AGENT_PROXY_LISTEN=off ZYVOR_AGENT_FLUXVM_URL="http://127.0.0.1:$SIM_PORT" ZYVOR_AGENT_EGRESS_ADVERTISE_HOST=127.0.0.1 \
    ZYVOR_AGENT_STATE_DIR="$WORK/state" ZYVOR_AGENT_SNAPSHOT_DIR="$WORK/snap" ZYVOR_AGENT_CREDENTIALS_FILE="$WORK/creds.json" \
    GOOGLE_CLIENT_ID="$GOOGLE_CLIENT_ID" GOOGLE_CLIENT_SECRET="$GOOGLE_CLIENT_SECRET" RUST_LOG=warn "$BIN" >"$WORK/runtime.log" 2>&1 &
PIDS+=($!)
for _ in $(seq 1 100); do curl -sf -o /dev/null "http://127.0.0.1:$API_PORT/healthz" && break; sleep 0.2; done
curl -sf -o /dev/null "http://127.0.0.1:$API_PORT/healthz" || { echo "the runtime did not start:"; tail -20 "$WORK/runtime.log"; exit 1; }
API="http://127.0.0.1:$API_PORT"

for a in gmail-triage mail-compose calendar-agent; do
  FABRIC_AGENT_URL="$API" FABRIC_AGENT_TOKEN="$OP_TOKEN" node "$ROOT/sdk/agent-runtime/src/cli.js" pack deploy "$ROOT/examples/merion-agents/$a" >"$WORK/$a.out" 2>&1 \
    || { echo "could not deploy $a:"; tail -5 "$WORK/$a.out"; exit 1; }
done

USER_TOKEN="$(curl -fsS -X POST -H "Authorization: Bearer $OP_TOKEN" -H 'content-type: application/json' \
  -d '{"user_id":"demo","scopes":["read","run","approve"],"ttl_seconds":86400}' "$API/v1/user-tokens" | python3 -c 'import json,sys; print(json.load(sys.stdin)["token"])')"

# your connection: the token is read from the file and sent to the runtime, and is never printed
python3 - "$TOKEN_FILE" "$API" "$USER_TOKEN" <<'PY'
import json, sys, urllib.request
tok = open(sys.argv[1]).read().strip().split("=", 1)[1]
req = urllib.request.Request(sys.argv[2] + "/v1/connections/google", method="PUT", data=json.dumps({"refresh_token": tok}).encode(),
                             headers={"Authorization": "Bearer " + sys.argv[3], "content-type": "application/json"})
urllib.request.urlopen(req, timeout=30).read()
PY
echo "connected your Google account for user 'demo' (write-only: the runtime never shows the token again)"

# a stand-in phone
node "$ROOT/sdk/agent-runtime/src/phone-cli.js" keygen "$WORK/phone.key" p256 >/dev/null
node "$ROOT/sdk/agent-runtime/src/phone-cli.js" enrol "$WORK/phone.key" demo-phone \
  | curl -fsS -o /dev/null -X POST -H "Authorization: Bearer $OP_TOKEN" -H 'content-type: application/json' --data-binary @- "$API/v1/users/demo/devices"

i=0
for a in gmail-triage mail-compose calendar-agent; do
  KEEP_API="$API" KEEP_TOKEN="$USER_TOKEN" python3 "$SCRIPT_DIR/merion-chat.py" --agent "$a" --port "$((CHAT_PORT+i))" >"$WORK/chat-$a.log" 2>&1 &
  PIDS+=($!); i=$((i+1))
done
sleep 1

cat <<OUT

  ============================================================
   Your Google account, a simulated cell, a software phone key.
  ============================================================

  Open one of these and talk to the agent:

    http://127.0.0.1:$CHAT_PORT      gmail-triage   say: unread            (lists your unread inbox headers)
    http://127.0.0.1:$((CHAT_PORT+1))      mail-compose   say:  draft
                                                          to: you@example.com
                                                          subject: Hello from Keep

                                                          A test message.
                                       (use "send" instead of "draft" to send it)
    http://127.0.0.1:$((CHAT_PORT+2))      calendar-agent say: agenda   (or:  add / title: ... / start: 2026-10-01T19:00:00+02:00 / end: ...)

  Drafts, sends and new events wait for you. This terminal is the phone: it shows what the host read out of the request
  and asks. Reading is not gated.

  Ctrl-C stops everything and deletes the temporary state.

OUT
KEEP_API="$API" KEEP_TOKEN="$USER_TOKEN" python3 "$SCRIPT_DIR/merion-approve.py" --key "$WORK/phone.key" --device demo-phone
