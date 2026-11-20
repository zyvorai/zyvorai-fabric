#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0
# Poll Hubble-lite flows. --max-seconds makes it testable.
set -euo pipefail
HOST="${FABRIC_HOST:-http://127.0.0.1:3000}"
MAX="${1:-2}"
I=0
while [ "$I" -lt "$MAX" ]; do
  curl -fsS "${HOST}/api/dataplane/hubble/flows?limit=16" || echo '{"items":[]}'
  I=$((I + 1))
  [ "$I" -lt "$MAX" ] && sleep 1
done
