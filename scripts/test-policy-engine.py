#!/usr/bin/env python3
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0
"""Offline tests for VM-edge policy_control (mirrors policy_control.rs)."""

from __future__ import annotations

import pathlib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]


def parse_v4(s: str) -> int | None:
    p = s.split(".")
    if len(p) != 4:
        return None
    n = 0
    for x in p:
        try:
            o = int(x)
        except ValueError:
            return None
        if o < 0 or o > 255:
            return None
        n = (n << 8) | o
    return n


def cidr_contains(cidr: str, ip: str) -> bool:
    host = ip.split("/")[0]
    if ":" in cidr or ":" in host:
        return cidr.split("/")[0] == host or cidr == host
    net, _, plen_s = cidr.partition("/")
    plen = 32 if not plen_s else min(32, int(plen_s))
    n = parse_v4(net)
    a = parse_v4(host)
    if n is None or a is None:
        return cidr == host
    if plen == 0:
        return True
    mask = 0xFFFFFFFF if plen == 32 else (~((1 << (32 - plen)) - 1)) & 0xFFFFFFFF
    return (n & mask) == (a & mask)


def host_cidr(addr: str) -> str:
    a = addr.split("/")[0].strip()
    if "." in a and "/" not in addr:
        return f"{a}/32"
    return addr.strip()


def explain(policy: dict, dest: str, port: int, proto: str) -> dict:
    proto = proto.lower()
    token = f"{proto}/{port}"
    matched_deny = next((c for c in policy.get("deny_cidrs", []) if cidr_contains(c, dest)), None)
    matched_allow = next((c for c in policy.get("allow_cidrs", []) if cidr_contains(c, dest)), None)
    ports = policy.get("allow_ports", [])
    port_ok = (not ports) or proto in ("icmp", "any") or any(p.lower() == token for p in ports)
    verdict, reason, would = "FORWARDED", "NONE", False
    if matched_deny:
        verdict, reason, would = "DROPPED", "POLICY_DENIED", True
    elif policy.get("allow_cidrs") and not matched_allow and not policy.get("default_allow"):
        verdict, reason, would = "DROPPED", "DEFAULT_DENY", True
    elif not port_ok and not policy.get("default_allow"):
        verdict, reason, would = "DROPPED", "PORT_DENIED", True
    elif not policy.get("default_allow") and not policy.get("allow_cidrs") and not ports:
        verdict, reason, would = "DROPPED", "DEFAULT_DENY", True
    if policy.get("audit_mode") and would:
        verdict, reason = "AUDIT", "AUDIT_WOULD_DROP"
    return {"verdict": verdict, "reason": reason, "would_drop": would}


class Engine(unittest.TestCase):
    def test_cidr(self):
        self.assertTrue(cidr_contains("10.0.0.0/8", "10.1.2.3"))
        self.assertFalse(cidr_contains("10.0.0.0/8", "11.0.0.1"))
        self.assertEqual(host_cidr("8.8.8.8"), "8.8.8.8/32")

    def test_explain_matrix(self):
        p = {
            "default_allow": False,
            "allow_cidrs": ["10.0.0.0/8"],
            "deny_cidrs": ["10.66.0.0/16"],
            "allow_ports": ["tcp/443"],
        }
        self.assertEqual(explain(p, "10.66.1.1", 443, "tcp")["reason"], "POLICY_DENIED")
        self.assertEqual(explain(p, "10.1.1.1", 22, "tcp")["reason"], "PORT_DENIED")
        self.assertEqual(explain(p, "10.1.1.1", 443, "tcp")["verdict"], "FORWARDED")
        p2 = dict(p, audit_mode=True)
        self.assertEqual(explain(p2, "1.1.1.1", 443, "tcp")["reason"], "AUDIT_WOULD_DROP")

    def test_dry_run_open_to_guard(self):
        open_p = {"default_allow": True, "allow_cidrs": [], "deny_cidrs": [], "allow_ports": []}
        guarded = dict(open_p, default_allow=False)
        r = explain(guarded, "1.1.1.1", 443, "tcp")
        self.assertTrue(r["would_drop"])
        self.assertEqual(r["reason"], "DEFAULT_DENY")

    def test_sources(self):
        self.assertTrue((ROOT / "backend/crates/driver-core/src/policy_control.rs").exists())
        self.assertTrue((ROOT / "web/src/lib/policyControls.ts").exists())
        self.assertFalse((ROOT / "backend/crates/driver-core/src/policy_engine.rs").exists())
        srv = (ROOT / "backend/zyvor-fabricd/src/server.rs").read_text()
        for path in ("policy/control", "dataplane/explain", "dataplane/dry-run", "dataplane/templates"):
            self.assertIn(path, srv)
        ui = (ROOT / "web/src/components/CiliumFlowControls.tsx").read_text()
        self.assertIn("Dry-run Guard", ui)
        self.assertIn("Explain", ui)
        cli = (ROOT / "backend/fabricctl/src/cli.rs").read_text()
        self.assertIn("DataplaneCmd::Explain", cli)
        self.assertIn("DataplaneCmd::DryRun", cli)


if __name__ == "__main__":
    unittest.main()
