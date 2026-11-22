#!/bin/sh
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0
# Stand-in for the Claude Code CLI. The first turn asks for approval.
# After the operator decision is written into PROMPT.md, the next turn exits.
prompt="${ZYVOR_HARNESS_WORKSPACE:-/opt/zyvor/agent/workspace}/PROMPT.md"
if grep -q "Operator decision" "$prompt" 2>/dev/null; then
  exit 0
fi
echo "ZYVOR_APPROVAL ship it?"
exit 0
