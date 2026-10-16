#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: Apache-2.0
# ============================================================================
# keep-demo-local — try Keep's use cases on this laptop in two minutes. NOT SEALED.
# ============================================================================
#   ./scripts/merion-demo-local.sh            # start the simulator and the runtime, print how to connect, run until Ctrl-C
#   ./scripts/merion-demo-local.sh --dry-run  # check the tools and print the plan
#
# Needs: python3, node 20+, curl, and either cargo (to build the runtime once) or KEEP_RUNTIME_BIN. No KVM, no Docker, no root.
#
# THIS IS A SIMULATOR. There is no VM and no network policy: the fixed extractors run as ordinary processes on this machine, so nothing here is
# isolated and the "0 connections" number is not evidence. Every result says "SIMULATED, not sealed" (evidence class `simulated`), and Solvor
# and the console show that instead of the proof. It exists so you can see what the use cases give you before you set up a Keep host
# (scripts/merion-up.sh). Never feed it files you would not run a script on.
# ============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BUILD=0
if [[ -n "${KEEP_RUNTIME_BIN:-}" ]]; then BIN="$KEEP_RUNTIME_BIN"; else BIN="$ROOT/agent-runtime/target/debug/zyvor-fabric-agent-runtime"; BUILD=1; fi
SIM_PORT="${KEEP_SIM_PORT:-17788}"
API_PORT="${KEEP_LOCAL_PORT:-19096}"
EGRESS_PORT="${KEEP_LOCAL_EGRESS_PORT:-18082}"
DRY=0
case "${1:-}" in
  --dry-run) DRY=1 ;;
  -h|--help) sed -n '5,17p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
  "") ;;
  *) echo "unknown option: $1" >&2; exit 64 ;;
esac

need() { command -v "$1" >/dev/null 2>&1 || { echo "missing: $1 ($2)" >&2; MISSING=1; }; }
MISSING=0
need python3 "install Python 3"
need curl "install curl"
need node "install Node 20 or newer: https://nodejs.org"
if (( BUILD )) || [[ ! -x "$BIN" ]]; then need cargo "install Rust from https://rustup.rs, or set KEEP_RUNTIME_BIN to a built runtime"; fi
(( MISSING == 0 )) || exit 1
if command -v node >/dev/null 2>&1 && (( $(node -p 'process.versions.node.split(".")[0]') < 20 )); then echo "node 20 or newer is required" >&2; exit 1; fi
for p in "$SIM_PORT" "$API_PORT" "$EGRESS_PORT"; do
  if (echo >"/dev/tcp/127.0.0.1/$p") 2>/dev/null; then echo "port $p is already in use (set KEEP_SIM_PORT / KEEP_LOCAL_PORT / KEEP_LOCAL_EGRESS_PORT)" >&2; exit 1; fi
done

echo "keep-demo-local: SIMULATOR, NOT SEALED (no VM, no network policy). Runtime at http://127.0.0.1:$API_PORT"
if (( DRY )); then
  echo "would: build the runtime if needed, start the simulator on :$SIM_PORT and the runtime on :$API_PORT with a random operator token,"
  echo "       mint a 1-day user token, print the Solvor settings and a first command, then wait until Ctrl-C and delete its temporary state"
  exit 0
fi

WORK="$(mktemp -d)"
PIDS=()
cleanup() { for p in ${PIDS[@]+"${PIDS[@]}"}; do kill "$p" 2>/dev/null || true; done; wait 2>/dev/null || true; rm -rf "$WORK"; echo; echo "keep-demo-local: stopped, temporary state removed"; }
trap cleanup EXIT INT TERM

# always an incremental build (a second or two when nothing changed), so the demo never runs a runtime older than this checkout
if (( BUILD )); then echo "building the runtime (incremental)..."; cargo build --manifest-path "$ROOT/agent-runtime/Cargo.toml" 2>&1 | tail -2; fi
[[ -x "$BIN" ]] || { echo "no runtime at $BIN" >&2; exit 1; }
[[ -d "$ROOT/sdk/agent-runtime/node_modules" ]] || npm install --prefix "$ROOT/sdk/agent-runtime" --no-audit --no-fund >/dev/null

OP_TOKEN="$(python3 -c 'import secrets; print(secrets.token_hex(32))')"
mkdir -p "$WORK/sandboxes" "$WORK/state" "$WORK/snap"
SANDBOX_STUB_PORT="$SIM_PORT" SANDBOX_STUB_ROOT="$WORK/sandboxes" python3 "$ROOT/agent-runtime/tests/sandbox_stub.py" >"$WORK/sim.log" 2>&1 &
PIDS+=($!)
env ZYVOR_AGENT_API_TOKEN="$OP_TOKEN" ZYVOR_AGENT_LISTEN="127.0.0.1:$API_PORT" ZYVOR_AGENT_EGRESS_LISTEN="127.0.0.1:$EGRESS_PORT" \
    ZYVOR_AGENT_PROXY_LISTEN=off ZYVOR_AGENT_FLUXVM_URL="http://127.0.0.1:$SIM_PORT" ZYVOR_AGENT_EGRESS_ADVERTISE_HOST=127.0.0.1 \
    ZYVOR_AGENT_STATE_DIR="$WORK/state" ZYVOR_AGENT_SNAPSHOT_DIR="$WORK/snap" RUST_LOG=warn "$BIN" >"$WORK/runtime.log" 2>&1 &
PIDS+=($!)
for _ in $(seq 1 100); do curl -sf -o /dev/null "http://127.0.0.1:$API_PORT/healthz" && break; sleep 0.2; done
curl -sf -o /dev/null "http://127.0.0.1:$API_PORT/healthz" || { echo "the runtime did not start:"; tail -20 "$WORK/runtime.log"; exit 1; }

# an agent to chat with (no model, no credentials): the simulator runs it like any other cell, unsigned because Keep mode is off here
FABRIC_AGENT_URL="http://127.0.0.1:$API_PORT" FABRIC_AGENT_TOKEN="$OP_TOKEN" node "$ROOT/sdk/agent-runtime/src/cli.js" pack deploy "$ROOT/examples/merion-agents/echo-agent" >"$WORK/echo-agent.out" 2>&1 \
  || { echo "could not deploy echo-agent:"; tail -5 "$WORK/echo-agent.out"; }
# an agent that uses your memory and goals (see docs/merion/memory/README.md): say "remember ..." to it
FABRIC_AGENT_URL="http://127.0.0.1:$API_PORT" FABRIC_AGENT_TOKEN="$OP_TOKEN" node "$ROOT/sdk/agent-runtime/src/cli.js" pack deploy "$ROOT/examples/merion-agents/memory-agent" >"$WORK/memory-agent.out" 2>&1 \
  || { echo "could not deploy memory-agent:"; tail -5 "$WORK/memory-agent.out"; }

USER_TOKEN="$(curl -fsS -X POST -H "Authorization: Bearer $OP_TOKEN" -H 'content-type: application/json' \
  -d '{"user_id":"demo","scopes":["read","run","approve"],"ttl_seconds":86400}' "http://127.0.0.1:$API_PORT/v1/user-tokens" \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)["token"])')"

cat <<OUT

  ============================================================
   SIMULATED, NOT SEALED. Files run as ordinary processes on
   this machine. Use only files you would run a script on.
  ============================================================

  Try a use case now (a sample file ships with each pack):

    KEEP_API=http://127.0.0.1:$API_PORT KEEP_TOKEN=$USER_TOKEN $ROOT/scripts/merion-demo.sh csv-clean
    KEEP_API=http://127.0.0.1:$API_PORT KEEP_TOKEN=$USER_TOKEN $ROOT/scripts/merion-demo.sh list

  Solvor (Mac app) finds this host by itself. Paste the token into its first screen, or copy this link and paste it there:

    keep://connect?host=http://127.0.0.1:$API_PORT&token=$USER_TOKEN
    (or: Settings, Host http://127.0.0.1:$API_PORT, Token above)
    or, in the Solvor repo:  make run KEEP_HOST=http://127.0.0.1:$API_PORT KEEP_TOKEN=$USER_TOKEN

  Chat with the example agent in a browser (a token stays on your machine; the page never sees it):

    KEEP_API=http://127.0.0.1:$API_PORT KEEP_TOKEN=$USER_TOKEN $ROOT/scripts/merion-chat.py --agent echo-agent      # then open http://127.0.0.1:8787
    (use --agent memory-agent to try Goals and Memory in the page: say "remember I like aisle seats" and review the suggestion under Memory)

  Solvor shows an amber "Simulated, not sealed" notice instead of the proof pill.
  For a real sealed cell, set up a host: scripts/merion-up.sh (needs Linux with KVM).

  Ctrl-C stops everything and deletes the temporary state.
OUT
wait
