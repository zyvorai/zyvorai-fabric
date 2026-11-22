#!/usr/bin/env python3
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0
"""Spec tests matching driver-core policy_control + web policyControls."""

from __future__ import annotations

import pathlib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]


def host_cidr(addr: str) -> str:
    a = addr.split("/")[0].strip()
    if ":" in a and "/" not in addr:
        return f"{a}/128"
    if "." in a and "/" not in addr:
        return f"{a}/32"
    return addr.strip()


def apply_mode(p: dict, mode: str) -> dict:
    n = dict(p)
    if mode == "open":
        n["default_allow"] = True
        n["audit_mode"] = False
    elif mode == "audit":
        n["audit_mode"] = True
        if not n.get("sample_rate"):
            n["sample_rate"] = 1
    else:
        n["default_allow"] = False
        n["audit_mode"] = False
        if not n.get("sample_rate"):
            n["sample_rate"] = 1
    return n


def invert(p: dict) -> dict:
    n = dict(p)
    n["allow_cidrs"], n["deny_cidrs"] = list(p.get("deny_cidrs", [])), list(p.get("allow_cidrs", []))
    n["default_allow"] = not p.get("default_allow", True)
    return n


class PolicyControls(unittest.TestCase):
    def test_guard_default_deny(self):
        p = apply_mode({"default_allow": True, "audit_mode": False, "sample_rate": 0}, "guard")
        self.assertFalse(p["default_allow"])
        self.assertFalse(p["audit_mode"])
        self.assertGreaterEqual(p["sample_rate"], 1)

    def test_invert_swaps(self):
        p = invert(
            {
                "default_allow": True,
                "allow_cidrs": ["10.0.0.0/8"],
                "deny_cidrs": ["1.1.1.1/32"],
            }
        )
        self.assertFalse(p["default_allow"])
        self.assertEqual(p["allow_cidrs"], ["1.1.1.1/32"])
        self.assertEqual(p["deny_cidrs"], ["10.0.0.0/8"])

    def test_host_slash32(self):
        self.assertEqual(host_cidr("8.8.8.8"), "8.8.8.8/32")

    def test_sources_exist(self):
        self.assertTrue((ROOT / "backend/crates/driver-core/src/policy_control.rs").exists())
        ts = (ROOT / "web/src/lib/policyControls.ts").read_text()
        self.assertIn("invertPolicy", ts)
        self.assertIn("guard", ts)
        ui = (ROOT / "web/src/components/CiliumFlowControls.tsx").read_text()
        self.assertIn("Guard", ui)
        self.assertIn("Invert", ui)
        self.assertIn("Block", ui)
        api = (ROOT / "backend/zyvor-fabricd/src/api/vm_dataplane.rs").read_text()
        self.assertIn("dataplane_policy_control", api)
        routes = (ROOT / "backend/zyvor-fabricd/src/server.rs").read_text()
        self.assertIn("policy/control", routes)


if __name__ == "__main__":
    unittest.main()
