#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: Apache-2.0
#
# CI / local smoke for browser-agent template (no KVM, no image bake).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
T="$ROOT/agent-runtime/templates/browser-agent"

echo "==> browser-agent bake-smoke"
test -f "$T/spec.json"
test -f "$T/build.json"
test -f "$T/driver.mjs"
test -f "$T/example.mjs"
test -f "$T/package.json"
test -f "$T/chromium-cdp.service"
test -f "$T/browser-driver.service"

python3 - <<PY
import json
spec = json.load(open("$T/spec.json"))
assert spec["name"] == "browser-agent"
assert spec["backend"] == "qemu"
assert spec.get("max_memory_mib") == 7900
build = json.load(open("$T/build.json"))
assert "chromium" in build["packages"]
assert "chromium-driver" in build["packages"]
assert build.get("pins", {}).get("playwright-core") == "1.49.1"
assert build.get("pins", {}).get("node") == "v20.18.1"
print("json ok")
PY

# Syntax-check driver + example with node if available
if command -v node >/dev/null; then
  node --check "$T/driver.mjs"
  node --check "$T/example.mjs"
  echo "node --check ok"
else
  echo "NOTE: node not on PATH — skip --check"
fi

# Driver must not expose evaluate/content to the tool surface
if grep -E 'page\.evaluate|page\.content\(' "$T/driver.mjs" | grep -v '^//' >/dev/null; then
  echo "FAIL: driver.mjs must not call page.evaluate / page.content" >&2
  exit 1
fi
echo "PASS  no evaluate/content in driver"
echo "OK — browser-agent bake-smoke"
