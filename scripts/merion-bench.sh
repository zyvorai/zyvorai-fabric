#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: Apache-2.0
#
# Measure a live Keep shard: how long a real cell run takes end to end, and how that holds up as more
# run at once. Every run boots a fresh cell from the template and extracts, so these are COLD numbers.
# (Needs a runtime that ends a use-case session when its run finishes, so the cell is released; on an older
# runtime the cells linger for 30 minutes and will exhaust the host.) Sizing a fleet from anything else is a guess.
#
#   export MERION_API=http://127.0.0.1:9096 MERION_TOKEN=...
#   ./scripts/merion-bench.sh                          # 6 runs at concurrency 1, 2, 4
#   ./scripts/merion-bench.sh --runs 10 --concurrency "1 2 4 8"
#
# Run it ON the shard host to get memory figures too (it reads /proc/meminfo). It refuses a concurrency
# level whose cells would need more than 60% of the memory that is available, so it cannot take the host down.
# What it does not measure: warm-pool starts (agent sessions report `startup_ms`), hibernate/resume, or
# a long soak. It says so rather than guessing.
# Pre-rename names still work: KEEP_<X> is used when MERION_<X> is unset.
[ -n "${MERION_API+x}" ] || [ -z "${KEEP_API:-}" ] || export MERION_API="$KEEP_API"
[ -n "${MERION_TOKEN+x}" ] || [ -z "${KEEP_TOKEN:-}" ] || export MERION_TOKEN="$KEEP_TOKEN"

set -uo pipefail

RUNS=6
LEVELS="1 2 4"
USE_CASE="csv-clean"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --runs) RUNS=$2; shift 2 ;;
    --concurrency) LEVELS=$2; shift 2 ;;
    --use-case) USE_CASE=$2; shift 2 ;;
    -h|--help) sed -n '2,16p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown option: $1" >&2; exit 64 ;;
  esac
done
: "${MERION_API:=${ZYVOR_AGENT_URL:-http://127.0.0.1:9096}}"
AUTH=()
[[ -n "${MERION_TOKEN:-${ZYVOR_AGENT_TOKEN:-}}" ]] && AUTH=(-H "Authorization: Bearer ${MERION_TOKEN:-$ZYVOR_AGENT_TOKEN}")
command -v python3 >/dev/null || { echo "python3 is required" >&2; exit 1; }
curl -sf "${AUTH[@]}" "$MERION_API/healthz" >/dev/null || { echo "runtime not reachable at $MERION_API" >&2; exit 2; }

WORK="$(mktemp -d "${TMPDIR:-/tmp}/merion-bench.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT
printf 'name,qty\nAnn,1\nBob,2\nCy,3\n' > "$WORK/in.csv"

mem_avail_mib() { awk '/MemAvailable/ {print int($2/1024)}' /proc/meminfo 2>/dev/null; }
CELL_MIB=$(python3 - <<'PY'
import json, os
for p in ("/var/lib/fluxvm/templates/node22-agent/spec.json",):
    try: print(json.load(open(p)).get("memory_mib", 2048)); break
    except Exception: pass
else: print(2048)
PY
)
HAVE_MEM=0; [[ -r /proc/meminfo ]] && HAVE_MEM=1

echo "==> keep bench: $USE_CASE x $RUNS runs per level, levels: $LEVELS"
echo "    shard: $MERION_API   cell memory: ${CELL_MIB} MiB (template)   host stats: $([[ $HAVE_MEM == 1 ]] && echo yes || echo 'no (run on the host for memory figures)')"

one_run() { # writes "<seconds> <http>" to $1, and the response body to $1.body
  curl -s -o "$1.body" -w '%{time_total} %{http_code}\n' "${AUTH[@]}" -X POST -F "file=@$WORK/in.csv" "$MERION_API/v1/demos/$USE_CASE" > "$1"
}

RESULTS="$WORK/results.jsonl"
for c in $LEVELS; do
  if [[ $HAVE_MEM == 1 ]]; then
    need=$((c * CELL_MIB)); avail=$(mem_avail_mib)
    if (( need * 10 > avail * 6 )); then
      echo "  skip concurrency $c: needs ${need} MiB, only ${avail} MiB available (limit 60%)"
      continue
    fi
  fi
  rm -f "$WORK"/r.*
  low=$(mem_avail_mib); start=$(python3 -c 'import time; print(time.time())')
  # $RUNS runs at concurrency $c: c workers, each doing its share one after another.
  per=$(( (RUNS + c - 1) / c ))
  for w in $(seq 1 "$c"); do
    ( for i in $(seq 1 "$per"); do one_run "$WORK/r.$w.$i"; done ) &
  done
  # Sample the lowest free memory while they run.
  while [[ -n "$(jobs -rp)" ]]; do
    if [[ $HAVE_MEM == 1 ]]; then m=$(mem_avail_mib); (( m < low )) && low=$m; fi
    sleep 0.5
  done
  wait
  end=$(python3 -c 'import time; print(time.time())')
  python3 - "$WORK" "$c" "$start" "$end" "${low:-0}" "$CELL_MIB" "$HAVE_MEM" >> "$RESULTS" <<'PY'
import glob, json, sys
work, c, start, end, low, cell, have_mem = sys.argv[1], int(sys.argv[2]), float(sys.argv[3]), float(sys.argv[4]), int(sys.argv[5] or 0), int(sys.argv[6]), sys.argv[7] == "1"
times, bad, codes, sample = [], 0, {}, None
for f in glob.glob(work + "/r.*"):
    if f.endswith(".body"): continue
    parts = open(f).read().split()
    if len(parts) == 2 and parts[1] == "201": times.append(float(parts[0]))
    else:
        bad += 1
        code = parts[1] if len(parts) == 2 else "no-reply"
        codes[code] = codes.get(code, 0) + 1
        if sample is None:
            try: sample = open(f + ".body").read()[:200].replace("\n", " ")
            except OSError: sample = ""
times.sort()
def pct(p):
    if not times: return None
    return times[min(len(times) - 1, int(round(p / 100 * (len(times) - 1))))]
wall = end - start
out = {"concurrency": c, "ok": len(times), "failed": bad, "p50_s": pct(50), "p95_s": pct(95), "max_s": times[-1] if times else None,
       "wall_s": round(wall, 1), "runs_per_minute": round(len(times) / wall * 60, 1) if wall else None}
if bad: out["failure_codes"] = codes; out["sample_error"] = sample
if have_mem: out["lowest_available_mib"] = low
print(json.dumps(out))
PY
done

echo
python3 - "$RESULTS" "$CELL_MIB" <<'PY'
import json, sys
rows = [json.loads(l) for l in open(sys.argv[1])] if True else []
cell = int(sys.argv[2])
if not rows:
    print("no results"); sys.exit(1)
f = lambda v: "-" if v is None else ("%.1f" % v)
print("| concurrency | ok | failed | p50 s | p95 s | max s | runs/min | lowest free MiB |")
print("|---|---|---|---|---|---|---|---|")
for r in rows:
    print("| %d | %d | %d | %s | %s | %s | %s | %s |" % (r["concurrency"], r["ok"], r["failed"], f(r["p50_s"]), f(r["p95_s"]), f(r["max_s"]), f(r["runs_per_minute"]), r.get("lowest_available_mib", "-")))
for r in rows:
    if r.get("failure_codes"):
        print("  concurrency %d failures: %s. First error: %s" % (r["concurrency"], r["failure_codes"], r.get("sample_error") or "(no body)"))
print()
print("Each run is a cold cell boot + extract. The template gives a cell %d MiB; the host pays something different (the VMM and page cache):" % cell)
print("compare 'lowest free MiB' between levels to see what one more concurrent cell really costs on this host.")
print("Not measured: warm-pool starts, hibernate/resume, sustained load. Do not size a fleet from a short run.")
json.dump(rows, open("/dev/stderr", "w"))
PY
