#!/usr/bin/env python3
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0
"""Enhance client presentation decks with Zyvor Fabric positioning."""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DECKS = ROOT / "docs" / "client-presentations"

BRAND_CSS = """
  .brand-line { font-size: 1em; color: #f97316; margin-top: 24px; font-weight: 600; }
  .brand-line a { color: #f97316; text-decoration: none; }
"""

REPLACEMENTS = [
    (
        "Next-Generation VM Management Platform",
        "Systemd-Native Private Cloud Control Plane",
    ),
    (
        "Enterprise virtual machine management, simplified.",
        "VM operations fabric — clustering, networking, security, storage, HA, and GPU on Linux.",
    ),
    (
        "<p class=\"subtitle\">Enterprise VM management in a single binary</p>",
        "<p class=\"subtitle\">Private cloud control plane in a single binary</p>",
    ),
    (
        "Zyvor Fabric is a production-grade VM management platform built in Rust.",
        "Zyvor Fabric is a production-grade private cloud control plane built in Rust.",
    ),
    (
        "# That's it. Your VM platform is running.",
        "# That's it. Zyvor Fabric is running.",
    ),
    (
        "<span class=\"badge\">MIT Licensed</span>",
        "<span class=\"badge\">Zyvor Family</span>\n    <span class=\"badge\" style=\"background:#431407;color:#f97316;\">zyvor.dev</span>",
    ),
]

TITLE_FOOTER = (
    '  <p class="brand-line"><a href="https://zyvor.dev" target="_blank" rel="noopener">zyvor.dev</a>'
    " · Part of the Zyvor product family · © 2026</p>\n"
)


def inject_css(html: str) -> str:
    if ".brand-line" in html:
        return html
    return html.replace("</style>", BRAND_CSS + "</style>", 1)


def inject_title_footer(html: str) -> str:
    if "class=\"brand-line\"" in html:
        return html
    # First title-slide only
    return re.sub(
        r"(<div class=\"slide title-slide\">[\s\S]*?</div>\s*</div>)",
        lambda m: m.group(0).replace("</div>\n</div>", TITLE_FOOTER + "</div>\n</div>", 1)
        if "</div>\n</div>" in m.group(0)
        else m.group(0),
        html,
        count=1,
    )


def process(path: Path) -> bool:
    text = path.read_text(encoding="utf-8")
    original = text
    text = inject_css(text)
    for old, new in REPLACEMENTS:
        text = text.replace(old, new)
    if path.name == "01-executive-overview.html":
        text = inject_title_footer(text)
    if text != original:
        path.write_text(text, encoding="utf-8")
        return True
    return False


def main() -> int:
    changed = 0
    for path in sorted(DECKS.glob("*.html")):
        if process(path):
            print(f"enhanced: {path.name}")
            changed += 1
    # README
    readme = DECKS / "README.md"
    text = readme.read_text(encoding="utf-8")
    text = text.replace("# vmspawn Client Presentations", "# Zyvor Fabric Client Presentations")
    text = text.replace("vmspawn Client", "Zyvor Fabric Client")
    if "POSITIONING" not in text:
        text += "\n## Positioning\n\nSee [Product Positioning](../POSITIONING.md) for Zyvor Fabric vs. Machina messaging.\n"
    readme.write_text(text, encoding="utf-8")
    print("updated: README.md")
    return 0


if __name__ == "__main__":
    sys.exit(main())
