#!/usr/bin/env python3
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0
"""Rebrand user-facing vmspawn/zyvor-fabricd prose to Zyvor Fabric while preserving technical identifiers."""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

DOC_GLOBS = [
    "docs/**/*.md",
    "docs/**/*.html",
    "README.md",
    "QUICKSTART.md",
    "FEATURES.md",
    "CONTRIBUTING.md",
    "ansible/README.md",
    "operator/README.md",
    "terraform-provider/README.md",
    "web/src/components/ZyvorAbout.tsx",
]

PROTECTED = [
    "systemd-vmspawn(1)",
    "systemd-vmspawn",
    "zyvor-fabric-vm-driver",
    "zyvor-fabric-sdk",
    "fabricctl",
    "zyvor-fabricd-operator",
    "zyvor-fabricd-cleanup",
    "zyvor-fabricd-backup",
    "zyvor-fabricd.service",
    "zyvor-fabricd.toml",
    "zyvor-fabricd.env",
    "zyvor-fabricd.conf",
    "zyvor-fabricd.spec",
    "zyvor-fabricd.local",
    "zyvor_fabric_token",
    "zyvor-fabricd-saved-login",
    "vmspawnd_username",
    "zyvor-fabricd-theme",
    "zyvor-fabricd-recent-pages",
    "zyvor-fabricd-pinned-pages",
    "vmspawnd_migration_templates",
    "vmspawnd_favorites",
    "vmspawnd_datacenter",
    "zyvor_fabric_vm",
    "/etc/zyvor-fabricd/",
    "/var/lib/zyvor-fabricd/",
    "/usr/lib/zyvor-fabricd",
    "/run/zyvor-fabricd",
    "ZYVOR_FABRICD_",
    "cargo run --bin zyvor-fabricd",
    "cargo build --bin zyvor-fabricd",
    "--bin zyvor-fabricd",
    "journalctl -u zyvor-fabricd",
    "systemctl enable --now zyvor-fabricd",
    "systemctl start zyvor-fabricd",
    "systemctl stop zyvor-fabricd",
    "systemctl restart zyvor-fabricd",
    "systemctl status zyvor-fabricd",
    "systemctl enable zyvor-fabricd",
    "systemctl disable zyvor-fabricd",
    "pam.d/zyvor-fabricd",
    "configs/pam.d/zyvor-fabricd",
    "plugins/modules/vmspawnd_",
    "zyvor-fabricd-01",
    "zyvor-fabricd nft",
]


def stash_technical(text: str) -> tuple[str, list[str]]:
    slots: list[str] = []

    def put(value: str) -> str:
        slots.append(value)
        return f"\x00S{len(slots) - 1:04d}\x00"

    parts = re.split(r"(```[\s\S]*?```|`[^`\n]+`)", text)
    for i in range(0, len(parts), 2):
        chunk = parts[i]
        for pat in PROTECTED:
            while pat in chunk:
                chunk = chunk.replace(pat, put(pat), 1)
        parts[i] = chunk
    return "".join(parts), slots


def restore_technical(text: str, slots: list[str]) -> str:
    for i, val in enumerate(slots):
        text = text.replace(f"\x00S{i:04d}\x00", val)
    return text


def apply_rebrand(text: str) -> str:
    text = text.replace("github.com/ssahani/vmspawn", "github.com/zyvorai/fabric")
    text = text.replace("github.com/ssahani/zyvor-fabric", "github.com/zyvorai/fabric")
    text = text.replace("github.com/ssahani/terraform-provider-zyvor-fabricd",
                        "github.com/zyvorai/terraform-provider-zyvor-fabricd")
    text = text.replace("registry.terraform.io/ssahani/", "registry.terraform.io/zyvorai/")
    text = re.sub(r"\bssahani/vmspawn\b", "zyvorai/fabric", text)
    text = re.sub(r"\bssahani/zyvor-fabricd\b", "zyvorai/zyvor-fabricd", text)
    text = re.sub(r"\bssahani/zyvor-fabric\b", "zyvorai/fabric", text)
    text = re.sub(r"\bssahani/", "zyvorai/", text)

    text, slots = stash_technical(text)

    text = re.sub(r"\bvmspawn\b(?!ctl|-driver|-sdk)", "Zyvor Fabric", text)
    text = re.sub(
        r"\bvmspawnd\b(?!\s*(?:daemon|service|socket|\.toml|\.env|operator|nft|local|_|-01))",
        "Zyvor Fabric",
        text,
    )

    text = restore_technical(text, slots)

    text = text.replace("Zyvor Fabric Fabric", "Zyvor Fabric")
    text = text.replace("Zyvor Zyvor", "Zyvor")
    return text


def process_file(path: Path) -> bool:
    original = path.read_text(encoding="utf-8")
    updated = apply_rebrand(original)
    if updated != original:
        path.write_text(updated, encoding="utf-8")
        return True
    return False


def main() -> int:
    changed = 0
    seen: set[Path] = set()
    for pattern in DOC_GLOBS:
        for path in ROOT.glob(pattern):
            if not path.is_file() or path in seen:
                continue
            seen.add(path)
            if process_file(path):
                print(f"updated: {path.relative_to(ROOT)}")
                changed += 1
    print(f"Done. {changed} file(s) updated.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
