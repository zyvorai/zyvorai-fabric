#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0
set -euo pipefail
HOST="${FABRIC_HOST:-http://127.0.0.1:3000}"
echo "dataplane-doctor host=$HOST"
curl -fsS "${HOST}/api/dataplane/health" >/tmp/dataplane-health.json || {
  echo "health endpoint unreachable (offline ok for unit wrap)"
  echo '{"ok":false,"notes":["offline"]}' >/tmp/dataplane-health.json
}
python3 - <<'PY'
import json, pathlib
p = pathlib.Path("/tmp/dataplane-health.json")
doc = json.loads(p.read_text())
print("ok=", doc.get("ok"), "mode=", doc.get("mode"), "notes=", doc.get("notes"))
print("dataplane-doctor: parsed")
PY
