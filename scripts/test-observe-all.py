#!/usr/bin/env python3
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

from __future__ import annotations

import json
import pathlib
import subprocess
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]


def parse_v4(s: str):
    p = s.split(".")
    if len(p) != 4:
        return None
    n = 0
    for x in p:
        o = int(x)
        if o < 0 or o > 255:
            return None
        n = (n << 8) | o
    return n


def cidr_contains(cidr: str, ip: str) -> bool:
    host = ip.split("/")[0]
    net, _, plen_s = cidr.partition("/")
    plen = 32 if not plen_s else min(32, int(plen_s))
    n, a = parse_v4(net), parse_v4(host)
    if n is None or a is None:
        return False
    mask = 0xFFFFFFFF if plen == 32 else (~((1 << (32 - plen)) - 1)) & 0xFFFFFFFF
    return (n & mask) == (a & mask)


class ObserveAll(unittest.TestCase):
    def test_engine_files(self):
        # Observe helpers live in policy_control (no separate policy_observe module).
        pc = (ROOT / "backend/crates/driver-core/src/policy_control.rs").read_text()
        self.assertIn("ManagementLockout", pc)
        self.assertIn("filter_flows", pc)
        self.assertIn("policy_fingerprint", pc)
        self.assertIn("GuardTimer", pc)
        srv = (ROOT / "backend/zyvor-fabricd/src/server.rs").read_text()
        for p in ("dataplane/explain", "dataplane/dry-run", "dataplane/templates"):
            self.assertIn(p, srv)
        cli = (ROOT / "backend/fabricctl/src/cli.rs").read_text()
        self.assertIn("DataplaneCmd::Explain", cli)
        self.assertIn("DataplaneCmd::DryRun", cli)

    def test_explain_logic(self):
        self.assertTrue(cidr_contains("10.0.0.0/8", "10.9.1.1"))
        self.assertFalse(cidr_contains("10.0.0.0/8", "11.0.0.1"))

    def test_gitops_and_tf(self):
        cr = (ROOT / "examples/devops/gitops/dataplane-policy.yaml").read_text()
        self.assertIn("FluxVmNetworkPolicy", cr)
        self.assertIn("PacketFlowWatch", cr)
        tf = (ROOT / "examples/devops/terraform/dataplane.tf").read_text()
        self.assertIn("policy/control", tf)

    def test_timers(self):
        import os
        env = {**os.environ, "FABRIC_DATAPLANE_TIMER_DIR": "/tmp/zyvor-dataplane-timers-test"}
        r = subprocess.run(
            ["python3", str(ROOT / "scripts/dataplane-timers.py"), "set", "--vm", "t1", "--ttl", "0"],
            check=True,
            capture_output=True,
            text=True,
            env=env,
        )
        rec = json.loads(r.stdout)
        self.assertEqual(rec["vm"], "t1")
        r2 = subprocess.run(
            ["python3", str(ROOT / "scripts/dataplane-timers.py"), "expired", "--now", str(rec["expires_unix"] + 1)],
            check=True,
            capture_output=True,
            text=True,
            env=env,
        )
        found = json.loads(r2.stdout)
        self.assertTrue(any(x["vm"] == "t1" for x in found))

    def test_chaos_and_bundle_and_follow(self):
        subprocess.run(["bash", str(ROOT / "scripts/dataplane-chaos-failclosed.sh")], check=True)
        with tempfile.TemporaryDirectory() as td:
            subprocess.run(["bash", str(ROOT / "scripts/dataplane-bundle.sh"), td], check=True)
            self.assertTrue((pathlib.Path(td) / "manifest.json").exists())
        self.assertTrue((ROOT / "scripts/dataplane-follow.sh").exists())
        self.assertTrue((ROOT / "scripts/dataplane-doctor.sh").exists())

    def test_ui_explain(self):
        ui = (ROOT / "web/src/components/CiliumFlowControls.tsx").read_text()
        self.assertIn("Explain", ui)
        self.assertIn("Dry-run Guard", ui)
        api = (ROOT / "web/src/api/dataplane.ts").read_text()
        self.assertIn("explainDataplane", api)
        self.assertIn("dryRunDataplane", api)
        self.assertIn("listDataplaneEndpoints", api)
        edge = (ROOT / "web/src/pages/EdgeDataplane.tsx").read_text()
        self.assertIn("endpoints", edge)
        self.assertIn("identity_source", edge)


if __name__ == "__main__":
    unittest.main()
