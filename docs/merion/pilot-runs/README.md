# Merion 0.1 pilot

Labeled when [`./scripts/merion-pilot-gate.sh`](../../../scripts/merion-pilot-gate.sh) passes **twice** on a FluxVM host (happy + deny), with archived logs under `docs/merion/pilot-runs/`.

## What the gate proves

| Check | Result |
|---|---|
| Template required | Missing template → FAIL |
| Merion mode policy | Unsigned PUT refused; empty signers refuse start |
| Session + cockpit | `evidence_class: software-test`; restart recovers session |
| OOB approval | Webhook + approve **and** deny paths; no unapproved mutate |
| Packaged agents | `examples/merion-agents/` + goals/artifacts |

## Archive index

| Run | Template | Result |
|---|---|---|
| [20260924T141927Z](20260924T141927Z/SUMMARY.md) | `node22-agent` | Happy PASS · Deny PASS |
| [20260924T154950Z](20260924T154950Z/SUMMARY.md) | `node22-agent` | Happy PASS · Deny PASS |
| [20260924T182930Z](20260924T182930Z/SUMMARY.md) | `node22-fc` | Happy PASS · Deny PASS · `cell_backend=flux-vm` · `guest_worker=ok` |
| [20260924T190954Z](20260924T190954Z/SUMMARY.md) | `node22-agent` (lab) | Browser screenshot proof: view lists the tab, screenshot HTTP 200 (`keep-cdp-stub`, no input takeover) |

All runs report evidence class **software-test**.

## Guest worker note

QEMU `node22-agent` boots with a **musl-static** `fluxvm-guest-agent` (host glibc
builds fail inside the Jammy guest with `GLIBC_2.39 not found`). After that bake,
host `POST …/agent/ping` returns 200 and the gate can record `guest_worker=ok`.

Firecracker `node22-fc` needs a **flat ext4** rootfs (not the GPT cloud image):
Firecracker appends `root=/dev/vda`, which panics on a partitioned disk. Bake with:

```bash
./scripts/merion-bake-fc-rootfs.sh
```

The pilot gate prefers `node22-fc` when that template exists.

Latest FC archive: [20260924T182930Z](20260924T182930Z/) (`template=node22-fc`,
`cell_backend=flux-vm`, happy+deny PASS, `guest_worker=ok`).

## Honesty

Measured profile remains **software-test**. Merion 0.2 + hardware for unread-by-operator claims.
