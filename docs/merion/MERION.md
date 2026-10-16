# Keep

> New here? Start with the [Keep README](https://github.com/zyvorai/fabric/blob/main/docs/merion/README.md). This page is the full spec.

**Pitch:** *the agent gets a real computer; you keep the keys, the policy, and the right to leave.*

Keep is a personal workstation for an untrusted agent. Muse got the threat model right; Keep ships the open version Meta cannot: **you run it, you read it, you take it with you.**

Apache-2.0, same as FluxVM. Keep is a **product layer in Fabric** on FluxVM —
not a third repo and not a second VMM.

**Docs home:** `fabric/docs/merion/` (this tree).  
**CLI:** `fabric/scripts/merionctl` (alias-worthy as `merionctl` on PATH).  
**Runtime:** `fabric/agent-runtime` (Sentinel, vault, egress ask, browser, approvals).  
**Hypervisor:** FluxVM Phase 6 `security_profile` only.

## How to test (same as CI)

```bash
# From fabric repo root — no KVM required
cargo test --manifest-path agent-runtime/Cargo.toml --lib
cargo test --manifest-path agent-runtime/Cargo.toml policy -- --nocapture

# Full Keep end-to-end (live runtime + FluxVM stub + merionctl)
./scripts/merion-e2e.sh

# Lab live FluxVM proof (Keep 0.1 release gate)
./scripts/merion-live-lab.sh

# Runtime control-plane e2e (proxy / Sentinel / DLP / phone approvals)
cargo build --manifest-path agent-runtime/Cargo.toml --release
BIN=agent-runtime/target/release/zyvor-fabric-agent-runtime \
  ./agent-runtime/scripts/e2e-no-fluxvm.sh
```

GitHub Actions: [`.github/workflows/merion.yml`](../../.github/workflows/merion.yml)  
Hands-on: [Tutorial 16](../tutorials/16-merion-workstation.md) · [Tutorial 17 — PDF brief](../tutorials/17-merion-pdf-brief.md)  
Evidence and status: [Security profiles](SECURITY-PROFILES.md) · [Roadmap](ROADMAP.md)  
Production checklist: [PRODUCTION.md](PRODUCTION.md)  
FluxVM measured profiles (sibling repo): `./scripts/test-security-profiles.sh`

## Quiet part (read this first)

Until Keep 0.2 on real SNP/TDX hardware with a user-held wrapping key:

> **The host can still see a measured VM.**

Evidence class `software-test` must never be marketed as “the operator cannot read this.” Muse Secure VM has the same limit today; they put it in a footnote. We put it here.

## What Muse got right

Treat the model as compromised the moment it reads a webpage.

- One persistent Linux computer per person, not a chat session.
- Two domains on one box: untrusted agent cell vs host-side authority.
- Agent never sees real passwords; surrogates swap at the egress boundary.
- Approvals are capabilities bound to a connector, not a sentence in the chat.
- Browser driver sees an accessibility tree, not raw DOM + JS.

## Why Keep beats Muse on purpose

| | Muse | Keep |
|---|---|---|
| Where it runs | Meta cloud only | Laptop, mini-PC, FluxVM host, rented SNP/TDX — same API |
| Policy | Closed Sentinel | Signed `merion.policy.yaml` you can diff in git |
| Cell | Its own dedicated cloud VM; Sentinel on the same machine, kept apart at the system level (Meta) | Firecracker / KVM microVM via FluxVM; the network policy is applied by the host, outside the guest |
| Model | Married to Muse Spark | BYO model socket |
| Training | Trajectories may train after sanitization | Training default **off**; export needs a scoped token |
| Host eBPF | Not a tenant-owned pin you can show | FluxVM TC: `deny_udp` + gateway-only ports; cockpit CONNECT 0 |
| Proof on stage | Trust Meta’s story | Keep audit journal + FluxVM `drop_reasons` (PacketWolf optional) |
| Operator / confidential | Operator may open the VM | Confidential = no host recover; measured says so honestly |
| Leave | Hard | `merionctl pack` / `unpack` onto another FluxVM |
| Client surface | Fat client helper surface | vsock admin; no SSH to the agent |

**Stack:** Muse (closed cloud agent) → **Keep** (product) → **Fabric** (control plane) → **FluxVM** (cell + host eBPF).

Muse: agent computer in Meta’s cloud. Keep: same idea on *your* FluxVM — signed policy, and CONNECT 0 from Keep’s journal + FluxVM’s pin.

## Architecture

```
You (phone / laptop / YubiKey)
  |  wrapping key + approval channel
  v
+------------------------------------------------------------------+
| HOST (Linux + KVM) — FluxVM node you control                     |
|                                                                  |
|  [Keep Sentinel]  signed policy + egress ask/sentinel + host TC/eBPF (`deny_udp`) |
|       ^ sole egress + connector authority                        |
|       |                                                          |
|  [Vault / authd]  secrets sealed to your key or vTPM             |
|       | surrogate in, real secret only at approved egress        |
|       v                                                          |
|  [Keep Supervisor]  measured/confidential FluxVM guest           |
|     +----------------------------------------------------------+ |
|     | FIRECRACKER / QEMU MICROVM  (untrusted agent cell)       | |
|     |   agent runtime + tools + workspace                      | |
|     |   no raw secrets, no CAP_NET_ADMIN, no host fs           | |
|     |   brokered Chromium (a11y tree only)                     | |
|     +----------------------------------------------------------+ |
|                                                                  |
|  durable state: postgres/sqlite on host, not in the cell         |
+------------------------------------------------------------------+
```

Security profiles (FluxVM Phase 6):

| Profile | Evidence | Hardware attestation? |
|---|---|---|
| `standard` | none | no |
| `measured` | `software-test` | **never** |
| `confidential-snp` / `confidential-tdx` | `sev-snp` / `tdx` only after verified hardware run | gated |

## Keep 0.1 — shipped

1. **BYO model** — a manifest's `model_socket` names an OpenAI-compatible endpoint (Qwen, DeepSeek, GLM, a local server) that an agent calls with `ctx.model.chat()` through the egress broker, and that CLI agents are pointed at; it also travels with pack / unpack. One-click use cases can opt in to a host-side model step, gated by the vault and an approval. See [MODELS.md](MODELS.md) and [MODEL.md](MODEL.md).
2. **Signed YAML Sentinel** — Keep mode (`ZYVOR_AGENT_MERION_MODE=1`) fail-closed; `sentinel/merion.policy.yaml`.
3. **Firecracker / measured cell** — agent kernel ≠ host kernel intent; `security_profile: measured` → evidence `software-test`.
4. **Phone-only high-risk approvals** — buy / send / delete via webhook / `/v1/approvals`, never in chat.
5. **Pack / unpack** — `merionctl pack` → USB or S3 → `merionctl unpack` on another FluxVM node.
6. **Cockpit + browser live view** — visible taint, last decisions, tab listing (`/keep/browser`), screenshot + read-only screencast; input takeover not implemented.
7. **One-click use cases** — `/app/merion` + `merion-demo.sh <id>`: PDF brief, contract clauses, security questionnaire, meeting actions, log triage, SBOM summary, CSV cleanup; each expects `egress_connects: 0` ([demos/README.md](demos/README.md)).
8. **Host eBPF pin (FluxVM)** — `deny_udp` + gateway-only ports; no PacketWolf required ([confine.md](confine.md)).

Lab gate: `./scripts/merion-live-lab.sh`. Guest boot needs a FluxVM template (Tutorial 11).

## Keep 0.2 — Muse’s “later,” without the wait-as-product

- User-held unwrap (phone / YubiKey) onto an attested guest.
- Attestation receipt on the phone: image hash, profile achieved, evidence class.
- Confidential profiles: **no** host recover path.
- Flip FluxVM `security.snp_launch_verified` / `tdx_launch_verified` only after one hardware run.

## What not to add

- Approvals inside the agent chat
- Helper-app Messages/Notes/file slurp as default
- Silent training on trajectories
- `extra_args` counting as confidential
- A second control plane besides FluxVM

## Layout (in Fabric — no third repo)

```
fabric/docs/merion/
  KEEP.md           # pitch, Muse deltas, honesty clause
  KEEP-0.2.md       # hardware gate
  STATUS.md
  confine.md        # FluxVM host eBPF (deny_udp)
  demos/            # one-click use cases (pdf-brief, contract-clauses, log-triage, ...)
  sentinel/         # merion.policy.yaml example
  vault/ cell/ browser/ approve/ cockpit/
fabric/scripts/merionctl
fabric/scripts/merion-demo-pdf.sh
fabric/agent-runtime/   # policy, model_socket, cockpit API, export-token, demos
```

## Related code

- FluxVM: https://github.com/zyvorai/fluxvm — Phase 6 `security_profile` (hypervisor only)
- Fabric: agent-runtime + **`fabricctl keep` / `merionctl`** + `docs/merion/` (this product surface)
- Design precursor: `fabric/docs/design/confidential-agent-vms.md`

There is no separate Keep git repository.
