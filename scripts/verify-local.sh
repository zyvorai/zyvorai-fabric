#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0
# Local verification — run before push when GitHub Actions billing is unavailable.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "==> Web typecheck"
(cd web && npm run typecheck)

echo "==> Web tests"
(cd web && npm test)

echo "==> Web build"
(cd web && npm run build)

echo "==> Backend fmt"
(cd backend && cargo fmt -- --check)

echo "==> zyvor-fabric-sdk clippy"
(cd backend && cargo clippy -p zyvor-fabric-sdk -- -D warnings)

echo "==> Terraform provider"
(cd terraform-provider && go mod tidy && go build ./... && go vet ./...)

echo "==> Operator"
(cd operator && cargo test)

echo "==> OpenAPI tier-1 coverage"
python3 scripts/check-openapi-coverage.py

echo "==> All local checks passed"
