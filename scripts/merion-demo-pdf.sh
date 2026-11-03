#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: Apache-2.0
#
# One-click PDF → brief.md demo (no browser, expect 0 CONNECT).
# Thin wrapper over merion-demo.sh, kept so Tutorial 17 and old notes still work.
exec "$(cd "$(dirname "$0")" && pwd)/merion-demo.sh" pdf-brief "$@"
