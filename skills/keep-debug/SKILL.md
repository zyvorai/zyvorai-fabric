---
name: keep-debug
description: Diagnose why a Keep cell was blocked, froze, or made an unexpected connection. Use when a run fails closed, exits 2, shows egress_connects above zero, or an agent is paused with ebpf_deny.
---

# Debug a Keep cell

References: `docs/merion/confine.md`, `docs/merion/merionctl/README.md`, `docs/merion/THREAT-MODEL.md`.

## Triage

1. `merionctl audit <session-uuid> --limit 50`. Look for `egress.connect` and `ebpf.*` rows. Confirm the hash-chain check on stderr passes.
2. Open the cockpit for the session: `merionctl cockpit <session-uuid>`. Check `egress_connects` and, if present, `drop_reasons`.
3. `merionctl policy show <agent>` and compare the denied host and method to the policy.

## What the symptoms mean

| Symptom | Meaning |
|---|---|
| `merionctl run` exits 2 | The cell made an outbound connection. |
| `egress_connects: 0` | The cell did not use the egress broker. It does not prove nothing else was reachable; the host policy is what stops that. |
| Run fails with 502 and the cell is deleted | The host could not apply the network policy, so it failed closed. |
| Agent paused, reason `ebpf_deny` | The host dataplane saw a denied connection and froze the session. |
| Drop reason `udp-deny` | UDP (QUIC, WebRTC, STUN) is blocked by design. |
| Guest cannot reach the network at all | Expected for use-case cells: they run over vsock with everything denied. Agent sessions need a host with agent networking. |

## The agent needed a host it was denied

Run `merionctl policy suggest <agent>` to see which hosts it asked for and could not reach. Loosening policy is the owner's decision: use the `keep-policy` skill to draft and review the change.

## Rules

- Do not loosen policy to make an error go away. Report what was blocked and let the owner decide.
- Do not print tokens or the admin password.
