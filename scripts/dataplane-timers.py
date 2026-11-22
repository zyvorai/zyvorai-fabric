#!/usr/bin/env python3
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0
"""Time-boxed Guard: store revert timers; expire back to open."""

from __future__ import annotations

import argparse
import json
import time
from pathlib import Path


def dir_path() -> Path:
    import os
    raw = os.environ.get("FABRIC_DATAPLANE_TIMER_DIR")
    if raw:
        return Path(raw)
    return Path("/tmp/zyvor-dataplane-timers")


def set_timer(vm: str, ttl: int, revert: str = "open") -> dict:
    d = dir_path()
    d.mkdir(parents=True, exist_ok=True)
    rec = {"vm": vm, "revert": revert, "expires_unix": int(time.time()) + ttl}
    (d / f"{vm}.json").write_text(json.dumps(rec, indent=2) + "\n")
    return rec


def expired(now: int | None = None) -> list[dict]:
    now = int(time.time() if now is None else now)
    d = dir_path()
    if not d.exists():
        return []
    out = []
    for p in d.glob("*.json"):
        rec = json.loads(p.read_text())
        if rec.get("expires_unix", 0) <= now:
            out.append(rec)
    return out


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("cmd", choices=["set", "expired"])
    ap.add_argument("--vm", default="web-1")
    ap.add_argument("--ttl", type=int, default=600)
    ap.add_argument("--now", type=int, default=None)
    args = ap.parse_args()
    if args.cmd == "set":
        print(json.dumps(set_timer(args.vm, args.ttl)))
    else:
        print(json.dumps(expired(args.now)))


if __name__ == "__main__":
    main()
