#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: Apache-2.0
#
# Re-record the README / Pages demos into docs/assets/demos/ (needs python3 + Pillow + ffmpeg).
#   ./scripts/record-demos.sh                 # record every demo
#   ./scripts/record-demos.sh keep-speculate  # one demo
#   RENDER_ONLY=1 ./scripts/record-demos.sh   # re-render from the committed .cast.jsonl files
#
# Each demo is a real run of a command (cast.py record) or, where the lab cannot run it, a scripted
# replay that says "illustrative" in its first frame. See docs/assets/demos/README.md.
set -euo pipefail
cd "$(dirname "$0")/.."
OUT=docs/assets/demos
CAST="python3 scripts/demo/cast.py"

# name | command that produces the output | command shown at the prompt | window title
DEMOS=(
  "keep-speculate|scripts/demo/run-speculate.sh|KEEP_E2E_DEMO=speculate-demo.sh ./scripts/keep-e2e.sh|keep: speculate, review, approve"
  "credential-broker|bash scripts/demo/illustrative-broker.sh|bash scripts/demo/illustrative-broker.sh|illustrative: credential broker"
)

for row in "${DEMOS[@]}"; do
  IFS='|' read -r name run shown title <<<"$row"
  if [[ $# -gt 0 ]]; then
    match=false; for n in "$@"; do [[ "$n" == "$name" ]] && match=true; done
    $match || continue
  fi
  echo "==> $name"
  if [[ -z "${RENDER_ONLY:-}" ]]; then
    cargo build --manifest-path agent-runtime/Cargo.toml --bins --examples >/dev/null 2>&1
    $CAST record "$OUT/$name.cast.jsonl" -- bash -c "$run"
  fi
  $CAST render "$OUT/$name.cast.jsonl" "$OUT/$name" --cmd "$shown" --title "$title"
done
