#!/usr/bin/env python3
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0
"""Append tier-1 GET paths from audit-ux-apis.sh into openapi.yaml."""
from __future__ import annotations

import re
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
AUDIT = REPO / "scripts" / "audit-ux-apis.sh"
OPENAPI = REPO / "backend" / "api-docs" / "openapi.yaml"

TAG_MAP = [
    ("/api/vms", "VMs"),
    ("/api/migrations", "Migrations"),
    ("/api/images", "Images"),
    ("/api/system/", "System"),
    ("/api/audit/", "Audit"),
    ("/api/networkd/", "Network"),
    ("/api/network/", "Network"),
    ("/api/firewall", "Security"),
    ("/api/nat-", "Network"),
    ("/api/qos-", "Network"),
    ("/api/dns-", "Network"),
    ("/api/vpn-", "Network"),
    ("/api/mirror-", "Network"),
    ("/api/monitor-", "Network"),
    ("/api/network-policies", "Security"),
    ("/api/identities", "Security"),
    ("/api/services", "Services"),
    ("/api/webhooks", "Integrations"),
    ("/api/jobs", "Jobs"),
    ("/api/pipeline/", "Jobs"),
    ("/api/compliance", "Compliance"),
    ("/api/certificates", "Certificates"),
    ("/api/billing/", "Billing"),
    ("/api/ft/", "HA"),
    ("/api/backups", "Backups"),
    ("/api/schedules", "Schedules"),
    ("/api/profiles", "VMs"),
    ("/api/zones", "HA"),
    ("/api/floating-ips", "Network"),
    ("/api/templates", "VMs"),
    ("/api/events", "Observability"),
    ("/api/users", "Auth"),
    ("/api/drs/", "HA"),
]


def tag_for(path: str) -> str:
    for prefix, tag in TAG_MAP:
        if path.startswith(prefix):
            return tag
    if path == "/health":
        return "Health"
    return "Dashboard"


def load_audit_paths() -> list[str]:
    text = AUDIT.read_text()
    block = re.search(r"ENDPOINTS=\((.*?)\)", text, re.S)
    if not block:
        raise SystemExit("ENDPOINTS block not found in audit script")
    return re.findall(r"([/\w.-]+)", block.group(1))


def path_exists(openapi: str, path: str) -> bool:
    return f"\n  {path}:\n" in openapi or f"\n  {path}:" in openapi


def yaml_path_block(path: str) -> str:
    tag = tag_for(path)
    summary = path.strip("/").replace("/", " · ").replace("-", " ")
    security = "      security: []\n" if path == "/health" else ""
    return f"""  {path}:
    get:
      summary: {summary}
      tags: [Tier-1, {tag}]
{security}      responses:
        '200':
          description: JSON payload
          content:
            application/json:
              schema:
                type: object
                additionalProperties: true
"""


def main() -> None:
    openapi = OPENAPI.read_text()
    paths = load_audit_paths()
    additions: list[str] = []
    for path in paths:
        if path_exists(openapi, path):
            continue
        additions.append(yaml_path_block(path))

    if not additions:
        print("OpenAPI already covers all audit tier-1 GET paths")
        return

    marker = "\ncomponents:"
    if marker not in openapi:
        raise SystemExit("components: section not found")
    openapi = openapi.replace(marker, "\n" + "".join(additions) + marker)
    OPENAPI.write_text(openapi)
    print(f"Added {len(additions)} tier-1 GET paths to {OPENAPI}")


if __name__ == "__main__":
    main()
