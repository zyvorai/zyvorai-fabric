#!/usr/bin/env python3
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0
"""Validate Fabric / FluxVM probe JSON against the sibling DevOps contract."""

from __future__ import annotations

from typing import Any


class ContractError(ValueError):
    pass


def require_bool(obj: dict[str, Any], key: str, where: str) -> bool:
    if key not in obj:
        raise ContractError(f"{where}: missing '{key}'")
    val = obj[key]
    if not isinstance(val, bool):
        raise ContractError(f"{where}: '{key}' must be bool, got {type(val).__name__}")
    return val


def check_fluxvm_healthz(status: int, body: dict[str, Any]) -> None:
    if status != 200:
        raise ContractError(f"fluxvm /healthz expected 200, got {status}")
    if not require_bool(body, "ok", "fluxvm /healthz"):
        raise ContractError("fluxvm /healthz ok must be true")


def check_fluxvm_readyz(status: int, body: dict[str, Any]) -> None:
    ok = require_bool(body, "ok", "fluxvm /readyz")
    if ok and status != 200:
        raise ContractError(f"fluxvm /readyz ok=true expected HTTP 200, got {status}")
    if not ok and status != 503:
        raise ContractError(f"fluxvm /readyz ok=false expected HTTP 503, got {status}")


def check_fabric_readyz(status: int, body: dict[str, Any]) -> None:
    for key in ("ok", "store", "fluxvm"):
        if key not in body:
            raise ContractError(f"fabric /readyz missing '{key}'")
    ok = require_bool(body, "ok", "fabric /readyz")
    store = require_bool(body, "store", "fabric /readyz")
    fluxvm = body["fluxvm"]
    if not isinstance(fluxvm, dict):
        raise ContractError("fabric /readyz fluxvm must be an object")
    flux_ok = fluxvm.get("ok")
    if flux_ok is None and "error" in fluxvm:
        flux_ok = False
    if not isinstance(flux_ok, bool):
        raise ContractError("fabric /readyz fluxvm.ok must be bool (or error object)")
    expected_ok = store and flux_ok
    if ok != expected_ok:
        raise ContractError(
            f"fabric /readyz ok={ok} but store={store} fluxvm.ok={flux_ok}"
        )
    if ok and status != 200:
        raise ContractError(f"fabric /readyz ok=true expected HTTP 200, got {status}")
    if not ok and status != 503:
        raise ContractError(f"fabric /readyz ok=false expected HTTP 503, got {status}")
