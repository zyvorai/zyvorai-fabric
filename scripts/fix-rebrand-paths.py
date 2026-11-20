#!/usr/bin/env python3
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0
"""Repair incorrect path replacements from rebrand-zyvor-fabric.py."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

GLOBS = [
    "docs/**/*",
    "README.md",
    "QUICKSTART.md",
    "FEATURES.md",
    "CONTRIBUTING.md",
    "ansible/README.md",
    "operator/README.md",
    "terraform-provider/README.md",
]

REPAIRS = [
    ("/etc/Zyvor Fabric", "/etc/zyvor-fabricd"),
    ("/var/lib/Zyvor Fabric", "/var/lib/zyvor-fabricd"),
    ("Zyvor Fabric/backend", "backend"),
    ("cd Zyvor Fabric", "cd zyvor-fabric"),
    ("systemd-Zyvor Fabric", "systemd-vmspawn"),
    ("useradd --system --home-dir /var/lib/zyvor-fabricd --shell /usr/sbin/nologin Zyvor Fabric", "useradd --system --home-dir /var/lib/zyvor-fabricd --shell /usr/sbin/nologin zyvor-fabricd"),
    ("chown -R Zyvor Fabric:Zyvor Fabric /var/lib/zyvor-fabricd", "chown -R zyvor-fabricd:zyvor-fabricd /var/lib/zyvor-fabricd"),
    ("chown Zyvor Fabric:Zyvor Fabric", "chown zyvor-fabricd:zyvor-fabricd"),
    ("sudo tar czf /tmp/Zyvor Fabric-pre-upgrade.tar.gz", "sudo tar czf /tmp/zyvor-fabricd-pre-upgrade.tar.gz"),
    ("Create `Zyvor Fabric/backend/configs/zyvor-fabricd.toml`", "Create `backend/configs/zyvor-fabricd.toml`"),
]


def main() -> int:
    changed = 0
    seen: set[Path] = set()
    for pattern in GLOBS:
        for path in ROOT.glob(pattern):
            if not path.is_file() or path.suffix not in {".md", ".html", ""}:
                if path.suffix not in {".md", ".html"}:
                    continue
            if path in seen:
                continue
            seen.add(path)
            text = path.read_text(encoding="utf-8")
            original = text
            for old, new in REPAIRS:
                text = text.replace(old, new)
            if text != original:
                path.write_text(text, encoding="utf-8")
                print(f"fixed: {path.relative_to(ROOT)}")
                changed += 1
    print(f"Done. {changed} file(s) fixed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
