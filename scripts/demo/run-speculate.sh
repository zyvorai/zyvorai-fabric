#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: Apache-2.0
# Start the Keep e2e environment quietly, then show only the narrated speculate demo.
# Pre-rename names still work: KEEP_<X> is used when MERION_<X> is unset.
[ -n "${MERION_E2E_DEMO+x}" ] || [ -z "${KEEP_E2E_DEMO:-}" ] || export MERION_E2E_DEMO="$KEEP_E2E_DEMO"

cd "$(dirname "$0")/../.."
MERION_E2E_DEMO=scripts/demo/speculate-demo.sh ./scripts/merion-e2e.sh 2>&1 |
  awk '/^=== DEMO/{p=1;next} p{print; fflush()}'
