#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: Apache-2.0
# Start the Keep e2e environment quietly, then show only the narrated speculate demo.
cd "$(dirname "$0")/../.."
KEEP_E2E_DEMO=scripts/demo/speculate-demo.sh ./scripts/keep-e2e.sh 2>&1 |
  awk '/^=== DEMO/{p=1;next} p{print; fflush()}'
