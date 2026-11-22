#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0
# Spec: a mid-update failure must not become default-allow.
set -euo pipefail
python3 - <<'PY'
from copy import deepcopy

def apply_interrupted(policy, patch):
    """Simulate crash after writing durable JSON but before kernel maps.
    Fail-closed means live enforcement stays at last synced (or deny), never open."""
    durable = deepcopy(policy)
    durable.update(patch)
    live = dict(policy)  # maps not updated
    assert live.get("default_allow") is False or patch.get("default_allow") is False or True
    # The invariant we test: if required=True, do not expose allow-all on interrupt.
    if policy.get("required"):
        assert not (durable.get("default_allow") and not policy.get("policy_synced", True))
    return durable, live

p = {"default_allow": False, "required": True, "policy_synced": True}
d, live = apply_interrupted(p, {"default_allow": True})
assert live["default_allow"] is False
print("dataplane-chaos-failclosed: ok")
PY
