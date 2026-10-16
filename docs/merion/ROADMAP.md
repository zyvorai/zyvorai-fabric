---
sidebar_position: 4
---

# Merion roadmap

Where each piece stands (what is left, in full: [REMAINING.md](REMAINING.md)). Sources: [STATUS.md](STATUS.md), [KEEP-0.2.md](MERION-0.2.md),
[browser/BROWSER-0.3.md](browser/BROWSER-0.3.md).

## Shipped

| Piece | What it is |
|---|---|
| FluxVM Phase 6 | `security_profile` field; `measured` profile produces `software-test` evidence |
| Merion 0.1 pilot | [Live gate](pilot-runs/README.md) passed twice (happy path and deny path) on a FluxVM host |
| Merion 0.1 | BYO model socket, signed Sentinel policy, measured cell, phone approvals, pack / unpack, cockpit, PDF brief, host eBPF pin |
| Merion Browser 0.3 | Split-sight pause, trajectory-as-code, origin taint lattice, vault-typed fill, goal-bound tabs, honesty badge |
| Policy and egress controls (2026-09-30) | Risk check on policy changes, drafts from denials, MCP/JSON-RPC/GraphQL body rules, per-program `binaries`, an operator egress guard hook, OCSF audit export, a seccomp filter in the inner container, GPU cells, credentials from a file or Vault, a Helm chart. All tested against fakes; not yet run in a real cell, on a real GPU or against a real Vault ([REMAINING.md](REMAINING.md)) |

## Planned product surface

Phased, in this order. All four phases are done.

1. **Run history and visibility** (shipped in this branch): artifact TTL and diff, run notifications, console and `merionctl` views.
2. **Triggers and batch** (done): signed webhook and watched-folder triggers, multi-file upload. See [TRIGGERS.md](TRIGGERS.md).
3. **More file types** (done): docx, xlsx, pptx, html, eml/mbox, zip fan-out, regex / JSON-path / table rules, and **OCR for photos and screenshots** (`ocr` extractor, English, needs `tesseract-ocr` in the cell template). Not built: OCR of scanned PDFs, other languages, HEIC.
4. **Model-assisted use cases** (done): a pack declares one model endpoint; the host makes the call, so the cell stays at 0 connections. See [MODEL.md](MODEL.md).

## Not planned here

- **A Mac or Windows desktop as the cell.** Merion cells are Linux microVMs on FluxVM. macOS guests are only permitted on
  Apple hardware and a Windows cell would need its own template, licensing and a desktop-control layer, so an agent that
  operates a Mac or Windows session is a separate project, not a use case. The [Mac and Windows packs](SCENARIOS.md#mac-and-windows-packs)
  read files exported from those machines instead.
- **Merion running on a Mac or Windows host.** FluxVM needs Linux/KVM.

## Gated on hardware

**Merion 0.2.** The soft scaffolding is complete (attestation receipt, no host recover on
confidential, user-held challenge API, browser screenshot and screencast). What remains
needs a real SNP/TDX run:

- User-held unwrap — the vault opens only after a phone or YubiKey unwraps a key onto an
  attested guest.
- Flip FluxVM `security.snp_launch_verified` / `tdx_launch_verified` after one hardware launch.

Until then: *the host can still see a measured VM.* See [Security profiles](SECURITY-PROFILES.md).

## Next (Browser 0.4 / 0.5)

- PacketWolf as an **optional** observer of the CONNECT 5-tuple (not required for Merion's proof).
- Signed site adapters (for example `adapters/vcenter.yaml`).
- Confidential cells: CDP only via attested vsock.

## What we will not add

- Approvals inside the agent chat
- Helper-app Messages / Notes / file slurp as a default
- Silent training on trajectories
