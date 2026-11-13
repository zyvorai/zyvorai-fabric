#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: Apache-2.0
# Print Merion attestation honesty: evidence stays software-test until FluxVM reports SNP/TDX verified.
#   ./scripts/merion-check-attestation.sh
set -euo pipefail
FLUXVM_URL="${MERION_FLUXVM_URL:-${ZYVOR_AGENT_FLUXVM_URL:-http://127.0.0.1:7788}}"
MERION_URL="${MERION_URL:-http://127.0.0.1:9096}"
echo "FluxVM: $FLUXVM_URL"
if caps=$(curl -fsS -m 5 "$FLUXVM_URL/v1/security/capabilities" 2>/dev/null); then
  echo "$caps" | python3 -c 'import json,sys; d=json.load(sys.stdin); print("  snp_launch_verified=", d.get("snp_launch_verified")); print("  tdx_launch_verified=", d.get("tdx_launch_verified"))'
else
  echo "  (FluxVM capabilities unreachable → treat as snp=false tdx=false)"
fi
echo "Merion: $MERION_URL"
if st=$(curl -fsS -m 5 "$MERION_URL/v1/keep/status" 2>/dev/null); then
  echo "$st" | python3 -c 'import json,sys
d=json.load(sys.stdin)
att=d.get("attestation") or d.get("security") or {}
print("  attestation=", json.dumps(att)[:400] if att else "(none on status)")
print("  evidence_class=software-test until snp/tdx launch verified on hardware")'
else
  echo "  (status unreachable)"
fi
echo "Done-when for Merion 0.2: record one verified SNP or TDX launch under docs/merion/pilot-runs/"
