#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: Apache-2.0
#
# Record the real Keep e2e run as docs/assets/merion/demo.gif (needs `vhs`:
# https://github.com/charmbracelet/vhs). Nothing is scripted or faked: the tape
# just types ./scripts/merion-e2e.sh and lets it run.
#
#   ./scripts/merion-record-demo.sh
#
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
command -v vhs >/dev/null || { echo "vhs not found (brew install vhs)" >&2; exit 1; }

# Warm the build so the recording shows the run, not the compile.
cargo build --manifest-path agent-runtime/Cargo.toml --bins --examples >/dev/null 2>&1

TAPE="$(mktemp "${TMPDIR:-/tmp}/keep-demo.XXXXXX.tape")"
trap 'rm -f "$TAPE"' EXIT
cat >"$TAPE" <<TAPE
Output "$ROOT/docs/assets/merion/demo.gif"
Set Shell "bash"
Set FontSize 16
Set Width 1100
Set Height 640
Set Theme "Dracula"
Set TypingSpeed 40ms
Set PlaybackSpeed 1.0
Type "./scripts/merion-e2e.sh 2>&1 | grep --line-buffered -E '^(==>|PASS|FAIL|passed|OK)'"
Sleep 500ms
Enter
Sleep 120s
TAPE

vhs "$TAPE"
[[ -f docs/assets/merion/demo.gif ]] || { echo "vhs finished but wrote no GIF (check ttyd and ffmpeg)" >&2; exit 1; }
echo "wrote docs/assets/merion/demo.gif ($(du -k docs/assets/merion/demo.gif | cut -f1) KB)"
