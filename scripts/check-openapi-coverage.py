#!/usr/bin/env python3
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0
"""Fail if audit tier-1 GET paths are missing from openapi.yaml."""
from __future__ import annotations

import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
AUDIT = REPO / "scripts" / "audit-ux-apis.sh"
OPENAPI = REPO / "backend" / "api-docs" / "openapi.yaml"


def load_audit_paths() -> list[str]:
    text = AUDIT.read_text()
    block = re.search(r"ENDPOINTS=\((.*?)\)", text, re.S)
    if not block:
        raise SystemExit("ENDPOINTS block not found")
    return re.findall(r"([/\w.-]+)", block.group(1))


def main() -> None:
    openapi = OPENAPI.read_text()
    missing = [p for p in load_audit_paths() if f"\n  {p}:\n" not in openapi]
    if missing:
        print(f"OpenAPI missing {len(missing)} audit paths:", file=sys.stderr)
        for p in missing:
            print(f"  - {p}", file=sys.stderr)
        sys.exit(1)
    print(f"OpenAPI covers all {len(load_audit_paths())} tier-1 audit GET paths")


if __name__ == "__main__":
    main()
