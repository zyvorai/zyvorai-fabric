#!/usr/bin/env python3
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0
"""Sanity-check DevOps example files (no live cluster)."""

from __future__ import annotations

import json
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent


class ExampleLayoutTests(unittest.TestCase):
    def test_required_files(self):
        for rel in (
            "github-actions/fabric-gates.yml",
            "gitlab-ci/fabric.yml",
            "gitops/kustomization.yaml",
            "gitops/virtualmachine.yaml",
            "terraform/main.tf",
            "ansible/site.yml",
            "apply-vm.yaml",
            "contract.py",
        ):
            path = ROOT / rel
            self.assertTrue(path.is_file(), rel)
            self.assertGreater(path.stat().st_size, 20, rel)

    def test_gitops_vm_api_version(self):
        text = (ROOT / "gitops/virtualmachine.yaml").read_text(encoding="utf-8")
        self.assertIn("apiVersion: zyvor-fabricd.io/v1alpha1", text)
        self.assertIn("kind: VirtualMachine", text)

    def test_terraform_uses_canonical_resource(self):
        text = (ROOT / "terraform/main.tf").read_text(encoding="utf-8")
        self.assertIn('source  = "zyvorai/zyvor-fabricd"', text)
        self.assertIn("resource \"zyvor-fabricd_vm\"", text)

    def test_apply_vm_has_tenant(self):
        text = (ROOT / "apply-vm.yaml").read_text(encoding="utf-8")
        self.assertIn("tenant: ci", text)

    def test_sibling_contract_json_parses(self):
        contract = ROOT.parents[1] / "docs/contracts/fabric-fluxvm-readyz.json"
        data = json.loads(contract.read_text(encoding="utf-8"))
        self.assertEqual(data["fluxvm"]["healthz"]["path"], "/healthz")
        self.assertEqual(data["fabric"]["readyz"]["path"], "/readyz")
        self.assertEqual(data["fluxvm"]["listen_default"], "http://127.0.0.1:7788")


if __name__ == "__main__":
    unittest.main()
