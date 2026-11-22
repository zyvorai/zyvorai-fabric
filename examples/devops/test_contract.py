#!/usr/bin/env python3
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

from __future__ import annotations

import unittest

from contract import (
    ContractError,
    check_fabric_readyz,
    check_fluxvm_healthz,
    check_fluxvm_readyz,
)


class FluxVmContractTests(unittest.TestCase):
    def test_healthz_ok(self):
        check_fluxvm_healthz(200, {"ok": True})

    def test_healthz_rejects_false(self):
        with self.assertRaises(ContractError):
            check_fluxvm_healthz(200, {"ok": False})

    def test_readyz_ok_and_not_ready(self):
        check_fluxvm_readyz(200, {"ok": True, "kvm": True, "state_dir": "/var/lib/fluxvm"})
        check_fluxvm_readyz(503, {"ok": False, "kvm": False})

    def test_readyz_status_mismatch(self):
        with self.assertRaises(ContractError):
            check_fluxvm_readyz(200, {"ok": False})


class FabricContractTests(unittest.TestCase):
    def test_ready_when_store_and_fluxvm_ok(self):
        check_fabric_readyz(
            200,
            {"ok": True, "store": True, "fluxvm": {"ok": True, "kvm": True}},
        )

    def test_not_ready_when_fluxvm_down(self):
        check_fabric_readyz(
            503,
            {"ok": False, "store": True, "fluxvm": {"error": "connection refused"}},
        )

    def test_rejects_lying_ok(self):
        with self.assertRaises(ContractError):
            check_fabric_readyz(
                200,
                {"ok": True, "store": True, "fluxvm": {"ok": False}},
            )

    def test_missing_fluxvm_key(self):
        with self.assertRaises(ContractError):
            check_fabric_readyz(200, {"ok": True, "store": True})


if __name__ == "__main__":
    unittest.main()
