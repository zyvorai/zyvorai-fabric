#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0
# Collect policy + last flows JSON into a support bundle directory.
set -euo pipefail
OUT="${1:-/tmp/fabric-dataplane-bundle}"
mkdir -p "$OUT"
echo '{"bundle":"dataplane","includes":["policy","flows","health"]}' >"$OUT/manifest.json"
echo "wrote $OUT/manifest.json"
