#!/usr/bin/env python3
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0
"""API / inventory / concurrency benchmark harness for Zyvor Fabric."""

from __future__ import annotations

import argparse
import json
import statistics
import sys
import time
import urllib.error
import urllib.request
from concurrent.futures import ThreadPoolExecutor, as_completed
from datetime import datetime, timezone
from typing import Any


def percentile(values: list[float], pct: float) -> float:
    if not values:
        return 0.0
    ordered = sorted(values)
    if len(ordered) == 1:
        return ordered[0]
    k = (len(ordered) - 1) * (pct / 100.0)
    lo = int(k)
    hi = min(lo + 1, len(ordered) - 1)
    frac = k - lo
    return ordered[lo] * (1.0 - frac) + ordered[hi] * frac


def summarize(latencies_ms: list[float], errors: int) -> dict[str, Any]:
    ok = latencies_ms
    return {
        "samples": len(ok) + errors,
        "ok": len(ok),
        "errors": errors,
        "p50_ms": round(percentile(ok, 50), 3) if ok else None,
        "p99_ms": round(percentile(ok, 99), 3) if ok else None,
        "mean_ms": round(statistics.fmean(ok), 3) if ok else None,
        "max_ms": round(max(ok), 3) if ok else None,
    }


def fetch(url: str, token: str | None, timeout: float) -> tuple[int, float, str]:
    headers = {"Accept": "application/json"}
    if token:
        headers["Authorization"] = f"Bearer {token}"
    req = urllib.request.Request(url, headers=headers, method="GET")
    start = time.perf_counter()
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            body = resp.read().decode("utf-8", errors="replace")
            elapsed = (time.perf_counter() - start) * 1000.0
            return resp.status, elapsed, body
    except urllib.error.HTTPError as exc:
        elapsed = (time.perf_counter() - start) * 1000.0
        return exc.code, elapsed, str(exc)
    except Exception as exc:  # noqa: BLE001 — harness must record any probe failure
        elapsed = (time.perf_counter() - start) * 1000.0
        return 0, elapsed, str(exc)


def run_series(url: str, token: str | None, iterations: int, timeout: float) -> dict[str, Any]:
    latencies: list[float] = []
    errors = 0
    last_status = None
    for _ in range(iterations):
        status, ms, _ = fetch(url, token, timeout)
        last_status = status
        if 200 <= status < 400:
            latencies.append(ms)
        else:
            errors += 1
    result = summarize(latencies, errors)
    result["url"] = url
    result["last_status"] = last_status
    return result


def run_concurrent(
    url: str, token: str | None, workers: int, iterations: int, timeout: float
) -> dict[str, Any]:
    latencies: list[float] = []
    errors = 0

    def one(_i: int) -> tuple[int, float]:
        status, ms, _ = fetch(url, token, timeout)
        return status, ms

    with ThreadPoolExecutor(max_workers=workers) as pool:
        futs = [pool.submit(one, i) for i in range(iterations)]
        for fut in as_completed(futs):
            status, ms = fut.result()
            if 200 <= status < 400:
                latencies.append(ms)
            else:
                errors += 1
    result = summarize(latencies, errors)
    result["url"] = url
    result["workers"] = workers
    return result


def probe_available(base_url: str, timeout: float) -> bool:
    status, _, _ = fetch(base_url.rstrip("/") + "/health", None, timeout)
    return 200 <= status < 500  # auth-on still counts as reachable


def build_report(args: argparse.Namespace, offline: bool) -> dict[str, Any]:
    report: dict[str, Any] = {
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "base_url": args.base_url,
        "status": "offline" if offline else "ok",
        "labeled_fluxvm_dependent": bool(args.label_fluxvm_dependent),
        "fluxvm_version": args.fluxvm_version,
        "iterations": args.iterations,
        "results": {},
    }
    if offline:
        report["note"] = (
            "Daemon not reachable. JSON is a schema placeholder, not a measurement."
        )
        return report

    health = args.base_url.rstrip("/") + "/health"
    ready = args.base_url.rstrip("/") + "/readyz"
    inventory = args.base_url.rstrip("/") + args.inventory_path
    report["results"]["health"] = run_series(health, args.token, args.iterations, args.timeout)
    report["results"]["readyz"] = run_series(ready, args.token, args.iterations, args.timeout)
    report["results"]["inventory"] = run_series(
        inventory, args.token, args.iterations, args.timeout
    )
    report["results"]["concurrent_health"] = run_concurrent(
        health, args.token, args.workers, args.iterations, args.timeout
    )
    return report


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    p = argparse.ArgumentParser(description="Zyvor Fabric benchmark harness")
    p.add_argument("--base-url", default="http://127.0.0.1:9095")
    p.add_argument("--token", default=None)
    p.add_argument("--iterations", type=int, default=50)
    p.add_argument("--workers", type=int, default=8)
    p.add_argument("--timeout", type=float, default=2.0)
    p.add_argument("--inventory-path", default="/api/vms")
    p.add_argument("--out", default=None)
    p.add_argument("--allow-offline", action="store_true")
    p.add_argument("--label-fluxvm-dependent", action="store_true")
    p.add_argument("--fluxvm-version", default=None)
    return p.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    offline = not probe_available(args.base_url, args.timeout)
    if offline and not args.allow_offline:
        print("error: daemon not reachable; pass --allow-offline for a placeholder", file=sys.stderr)
        return 2
    report = build_report(args, offline)
    text = json.dumps(report, indent=2)
    if args.out:
        with open(args.out, "w", encoding="utf-8") as fh:
            fh.write(text + "\n")
    print(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
