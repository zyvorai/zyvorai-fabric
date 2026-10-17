#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: Apache-2.0
# ============================================================================
# merion-watch — notice when a public web page changes, using a Keep use case to read it
# ============================================================================
#   MERION_API=http://127.0.0.1:9096 MERION_TOKEN=... ./scripts/merion-watch.sh https://status.example.com [options]
#
#   --use-case ID     the use case that reads the page (default status-page-watch; it must accept .html)
#   --match TEXT      alert ONLY when TEXT newly appears in the summary (for example "in stock"); other changes are shown but do not alert
#   --notify          show a desktop notification on a change (osascript on macOS, notify-send on Linux)
#   --state-dir DIR   where the last run is remembered (default ~/.local/state/merion-watch)
#   --max-failures N  after N failures in a row exit 4 instead of 2 (default 3)
#
# Run it from cron or launchd every few minutes. Exit codes: 0 unchanged (or the first run), 3 changed or matched, 2 the fetch or the run
# failed, 4 it has failed N times in a row.
#
# How it works, and what it does not do. THIS SCRIPT fetches the page, on YOUR machine's network, with a size and time limit; the sealed cell never
# touches the network. The saved page is then read by a Keep use case in a cell, and the change is the difference between this summary and the last one
# (the runtime's own /v1/artifacts/{a}/diff/{b}). It does not log in, run JavaScript, or click anything, and it does not fetch pages for you from the host:
# a public page only, as you would open it. Do not put a token or password in the URL.
# ============================================================================
# Pre-rename names still work: KEEP_<X> is used when MERION_<X> is unset.
[ -n "${MERION_API+x}" ] || [ -z "${KEEP_API:-}" ] || export MERION_API="$KEEP_API"
[ -n "${MERION_TOKEN+x}" ] || [ -z "${KEEP_TOKEN:-}" ] || export MERION_TOKEN="$KEEP_TOKEN"
[ -n "${MERION_WATCH_STATE+x}" ] || [ -z "${KEEP_WATCH_STATE:-}" ] || export MERION_WATCH_STATE="$KEEP_WATCH_STATE"

set -euo pipefail

: "${MERION_API:=http://127.0.0.1:9096}"
USE_CASE=status-page-watch; MATCH=""; NOTIFY=0; MAX_FAIL=3
STATE_DIR="${MERION_WATCH_STATE:-${XDG_STATE_HOME:-$HOME/.local/state}/merion-watch}"
URL=""
usage() { sed -n '5,20p' "$0" | sed 's/^# \{0,1\}//'; exit "${1:-0}"; }
while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help) usage 0 ;;
    --use-case) USE_CASE="${2:?--use-case needs an id}"; shift ;;
    --match) MATCH="${2:?--match needs text}"; shift ;;
    --notify) NOTIFY=1 ;;
    --state-dir) STATE_DIR="${2:?--state-dir needs a path}"; shift ;;
    --max-failures) MAX_FAIL="${2:?--max-failures needs a number}"; shift ;;
    -*) echo "unknown option: $1" >&2; usage 64 ;;
    *) [[ -z "$URL" ]] || { echo "one URL only" >&2; usage 64; }; URL="$1" ;;
  esac
  shift
done
[[ -n "$URL" ]] || usage 64
[[ "$URL" =~ ^https?://[^/@[:space:]]+(/[^[:space:]]*)?$ ]] || { echo "the URL must be http(s), with no user:password@ and no spaces" >&2; exit 64; }
[[ ${#URL} -le 2048 ]] || { echo "the URL is too long" >&2; exit 64; }
[[ "$USE_CASE" =~ ^[a-z0-9-]{1,48}$ ]] || { echo "bad use case id" >&2; exit 64; }
[[ "$MAX_FAIL" =~ ^[1-9][0-9]*$ ]] || { echo "--max-failures must be a positive number" >&2; exit 64; }
command -v curl >/dev/null && command -v python3 >/dev/null || { echo "curl and python3 are required" >&2; exit 1; }
AUTH=(); [[ -n "${MERION_TOKEN:-}" ]] && AUTH=(-H "Authorization: Bearer $MERION_TOKEN")

mkdir -p "$STATE_DIR"; chmod 700 "$STATE_DIR" 2>/dev/null || true
KEY="$(printf '%s' "$URL" | python3 -c 'import hashlib,sys; print(hashlib.sha256(sys.stdin.buffer.read()).hexdigest()[:16])')"
STATE="$STATE_DIR/$KEY.json"
W="$(mktemp -d)"; trap 'rm -rf "$W"' EXIT

state_get() { python3 -c 'import json,sys
try: print(json.load(open(sys.argv[1])).get(sys.argv[2], ""))
except Exception: print("")' "$STATE" "$1"; }
state_put() { python3 - "$STATE" "$URL" "$1" "$2" <<'PY'
import json, sys
path, url, last, failures = sys.argv[1:]
json.dump({"url": url, "last_artifact": last, "failures": int(failures)}, open(path, "w"))
PY
}
clean() { tr -d '\000-\010\013-\037' | cut -c1-200; }   # the summary is untrusted text headed for a terminal
notify() { (( NOTIFY )) || return 0
  if command -v osascript >/dev/null 2>&1; then osascript -e "display notification \"$(printf '%s' "$1" | tr -d '"\\')\" with title \"Keep watch\"" >/dev/null 2>&1 || true
  elif command -v notify-send >/dev/null 2>&1; then notify-send "Keep watch" "$1" >/dev/null 2>&1 || true; fi; }

LAST="$(state_get last_artifact)"; FAILS="$(state_get failures)"; FAILS="${FAILS:-0}"
fail() { # message
  FAILS=$((FAILS + 1)); state_put "$LAST" "$FAILS"
  echo "FAILED: $1 ($FAILS in a row)" >&2
  (( FAILS >= MAX_FAIL )) && { notify "$URL has failed $FAILS times in a row"; exit 4; }
  exit 2
}

# 1. fetch the page on this machine: http(s) only, no redirects to other schemes, 2 MiB and 30 s at most
curl -fsSL --proto '=http,https' --proto-redir '=http,https' --max-time 30 --max-filesize 2097152 -A 'merion-watch' -o "$W/page.html" -- "$URL" 2>"$W/curl.err" \
  || fail "could not fetch the page: $(clean < "$W/curl.err" | head -1)"
[[ -s "$W/page.html" ]] || fail "the page was empty"

# 2. read it in a sealed cell (a constant file name, so the summary does not change because of the name)
RESP="$(curl -sS -m 180 "${AUTH[@]}" -X POST -F "file=@$W/page.html" "$MERION_API/v1/demos/$USE_CASE" 2>"$W/run.err")" || fail "the use case run failed: $(clean < "$W/run.err" | head -1)"
NEW="$(python3 -c 'import json,sys
try: print(json.load(sys.stdin)["artifacts"][0]["id"])
except Exception: print("")' <<<"$RESP")"
[[ -n "$NEW" ]] || fail "the use case returned no result: $(printf '%s' "$RESP" | clean | head -1)"

# 3. first run: remember it
if [[ -z "$LAST" ]]; then
  state_put "$NEW" 0
  echo "BASELINE: first run recorded for $URL"
  exit 0
fi

# 4. compare with the last summary using the runtime's own diff
DIFF="$(curl -sS -m 60 "${AUTH[@]}" "$MERION_API/v1/artifacts/$LAST/diff/$NEW" 2>/dev/null || true)"
python3 - "$DIFF" "$W/diff.txt" <<'PY'
import json, sys
try:
    d = json.loads(sys.argv[1])
    lines, s = d["lines"], d["summary"]
except Exception:
    print("the previous result is no longer available; treating this run as the new baseline", file=sys.stderr)
    sys.exit(0)
if s["added"] or s["removed"]:
    out = [f"CHANGED: +{s['added']} -{s['removed']}"]
    out += [("+ " if l["op"] == "add" else "- ") + l["line"] for l in lines if l["op"] in ("add", "del")]
    open(sys.argv[2], "w").write("\n".join(out[:41]) + "\n")
PY
state_put "$NEW" 0
CHANGED=0; [[ -s "$W/diff.txt" ]] && CHANGED=1

# 5. --match: alert only when TEXT is newly in the summary (it was not there last time); other changes are shown but do not alert
if [[ -n "$MATCH" ]]; then
  body() { curl -sS -m 30 "${AUTH[@]}" "$MERION_API/v1/artifacts/$1" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("body",""))' 2>/dev/null || true; }
  if (( CHANGED )) && grep -qiF -- "$MATCH" <<<"$(body "$NEW")" && ! grep -qiF -- "$MATCH" <<<"$(body "$LAST")"; then
    echo "MATCH: \"$MATCH\" now appears for $URL"; clean < "$W/diff.txt"; notify "\"$MATCH\" now appears on $URL"; exit 3
  fi
  if (( CHANGED )); then echo "CHANGED, but \"$MATCH\" did not newly appear (not alerting):"; clean < "$W/diff.txt"; else echo "UNCHANGED: $URL"; fi
  exit 0
fi

if (( CHANGED )); then
  clean < "$W/diff.txt"
  notify "$(head -1 "$W/diff.txt") on $URL"
  exit 3
fi
echo "UNCHANGED: $URL"
exit 0
