# Tutorial 16: Merion — open workstation for an untrusted agent

Run a **Merion**: a personal Linux cell for an agent you do not have to trust,
with a Sentinel policy you can read, approvals out of band, and an honest
measured security profile.

**Level:** Intermediate  
**Time:** 45 minutes  
**Repos:** Fabric agent-runtime, FluxVM reachable, `curl`, `jq`, Node 20+ optional for bundles.

> **Honesty.** Until Merion 0.2 on real SNP/TDX with a user-held key, *the host
> can still see a measured VM.* Evidence class `software-test` must never be
> marketed as “the operator cannot read this.” See [docs/merion/MERION.md](../merion/MERION.md).

GitHub Actions runs the unit/policy suite + stub e2e without KVM:
[`.github/workflows/merion.yml`](../../.github/workflows/merion.yml).  
Lab live FluxVM gate: [`./scripts/merion-live-lab.sh`](../../scripts/merion-live-lab.sh) · [PRODUCTION.md](../merion/PRODUCTION.md).

---

## What you will learn

1. How Merion maps onto Fabric agent-runtime + FluxVM (no third repo).
2. How to run the CI suite and the Merion 0.1 live lab gate.
3. How to run **Merion mode** (fail-closed signed policy).
4. How to export/import `merion.policy.yaml` with `merionctl`.
5. How to open cockpit + browser live view.
6. How training stays off unless you mint an export token.

---

## Step 0 — Run the CI suite locally

```bash
cd fabric
cargo test --manifest-path agent-runtime/Cargo.toml --lib
cargo test --manifest-path agent-runtime/Cargo.toml policy -- --nocapture
./scripts/merion-e2e.sh
./scripts/merionctl --help
```

Expect ~143 lib tests green and stub e2e PASS. That is the same gate as the Merion workflow.

On a host with FluxVM up:

```bash
./scripts/merion-live-lab.sh
# Guest boot needs a registered template (Tutorial 11 node22-agent) or:
# MERION_E2E_TEMPLATE=node22-agent ./scripts/merion-live-lab.sh
```

---

## Step 1 — Start the agent runtime (Merion mode)

```bash
# Generate a one-off Ed25519 seed for this lab (32-byte hex)
SEED=$(python3 -c 'import os; print(os.urandom(32).hex())')
PUB=$(./agent-runtime/target/release/examples/merion_sign_policy pubkey "$SEED" \
  2>/dev/null || cargo run --manifest-path agent-runtime/Cargo.toml --example merion_sign_policy -- pubkey "$SEED")

export ZYVOR_AGENT_LISTEN=127.0.0.1:9096
export ZYVOR_AGENT_API_TOKEN=dev-token
export ZYVOR_AGENT_FLUXVM_URL=http://127.0.0.1:7788
export ZYVOR_AGENT_MERION_MODE=1
export ZYVOR_AGENT_POLICY_TRUSTED_SIGNERS="$PUB"
export ZYVOR_AGENT_CONFINE=1          # optional but recommended
export ZYVOR_AGENT_SECURITY_PROFILE=measured  # default under Merion mode
# optional: approval webhook your phone/CLI can reach
# export ZYVOR_AGENT_APPROVAL_WEBHOOK=https://…

cargo run --manifest-path agent-runtime/Cargo.toml --release
```

Merion mode **refuses to start** without trusted signers, and refuses unsigned
`PUT /v1/agents/{name}/policy`.

```bash
export MERION_API=http://127.0.0.1:9096
export MERION_TOKEN=dev-token
curl -sS -H "Authorization: Bearer $MERION_TOKEN" "$MERION_API/healthz"
```

---

## Step 2 — Deploy an agent (Merion cell)

Use Tutorial 11’s deploy JSON shape, and add Merion fields:

```json
{
  "name": "merion-demo",
  "bundle_base64": "<base64 of bundle.mjs>",
  "manifest": {
    "template": "node22-agent",
    "egress_mode": "ask",
    "egress_allow_hosts": ["api.github.com"],
    "confinement": "strict",
    "taint": { "trusted_hosts": ["api.github.com"] },
    "model_socket": {
      "base_url": "https://api.x.ai/v1",
      "model": "grok-4",
      "credential": "xai"
    },
    "cell_backend": "firecracker",
    "browser_port": 9222
  }
}
```

```bash
./scripts/merionctl create -f /tmp/merion-demo-deploy.json
```

`model_socket` is the BYO brain — swap Grok / local GGUF / vLLM without rebuilding the cell.  
`cell_backend: firecracker` records the Merion 0.1 intent (agent kernel ≠ host kernel); the live sandbox still follows FluxVM’s sandbox API today.  
Console UX: marketing `/keep` → **Open Agents** when signed in.

---

## Step 3 — Readable Sentinel policy (must be signed)

```bash
./scripts/merionctl policy show merion-demo > /tmp/merion.policy.yaml
# edit allow/deny/ask/taint, then sign and set:
./agent-runtime/target/release/examples/merion_sign_policy sign "$SEED" /tmp/merion.policy.yaml \
  > /tmp/merion.policy.yaml.sig
./scripts/merionctl policy set merion-demo /tmp/merion.policy.yaml /tmp/merion.policy.yaml.sig
```

Unsigned `PUT` returns **403** in Merion mode. Example schema:
[docs/merion/sentinel/merion.policy.yaml](../merion/sentinel/merion.policy.yaml).

High-risk actions (buy / send / delete) must go through `/v1/approvals` / your
webhook — never confirm inside the agent chat.

Credentials use host-side `authorize_resolve` allowlists
([vault/README.md](../merion/vault/README.md)); secrets remain host-readable until Merion 0.2.

---

## Step 4 — Cockpit + browser live view

After a session exists:

```bash
SID=<session-uuid>
./scripts/merionctl cockpit "$SID" | jq '{tainted_by, taint_visible, evidence_class, browser_view, honesty}'
# HTML (phone-friendly):
open "$MERION_API/merion/cockpit?session=$SID"
# Tab listing (needs browser_port on the agent):
curl -sS -H "Authorization: Bearer $MERION_TOKEN" \
  "$MERION_API/v1/sessions/$SID/browser/view" | jq .
open "$MERION_API/merion/browser?session=$SID"
```

Untrusted reads paint `tainted_by`; egress flips to ask. Screencast and input
takeover are **not** in Merion 0.1 — see [browser/README.md](../merion/browser/README.md).

---

## Step 5 — Pack / unpack (two homes, one disk story)

```bash
./scripts/merionctl pack   /tmp/keep-pack merion-demo
./scripts/merionctl unpack /tmp/keep-pack merion-demo
```

This moves policy + manifest pin. Copy the FluxVM qcow2 / vault export beside
the pack directory when you migrate hosts. Measured image pin travels with the
FluxVM catalog alias you used for `security_profile: measured` VMs.

---

## Step 6 — Training default off

```bash
# Refused without an explicit scoped token:
./scripts/merionctl export-token 'trajectory:read:7d' 3600
```

Sanitization footnotes do not count. No token → no trajectory leaves the box.

---

## Measured profile on FluxVM (optional on this host)

If the node has OVMF + swtpm + a signed catalog:

```bash
# On the FluxVM host — see zyvorai/fluxvm docs/guides/security-profiles-howto.md
curl -sS -X POST http://127.0.0.1:7788/v1/vms \
  -H 'Content-Type: application/json' \
  --data @examples/qemu-measured.json
```

Under Merion mode the agent-runtime defaults `ZYVOR_AGENT_SECURITY_PROFILE=measured`
on sandbox create. Cockpit still labels evidence `software-test`.

FluxVM CI for that path: `./scripts/test-security-profiles.sh` in the fluxvm repo.

---

## What not to do

- Approvals inside the agent chat  
- Calling `software-test` evidence “hardware attestation”  
- Treating QEMU `extra_args` as proof of confidential launch  
- Running production Merion without `ZYVOR_AGENT_MERION_MODE=1` and trusted signers  
- Opening a third Merion git repository — Merion lives in Fabric  

Merion 0.2 **soft** scaffolding (receipt, screencast, user-held challenge, fabricd
proxies) is already in tree — see [KEEP-0.2.md](../merion/MERION-0.2.md). Hardware
SNP/TDX + real user-held unwrap is still required before unread-by-operator claims.

---

## Appendix — Brokered browser (`browser-agent`)

Bake on a FluxVM host (Debian Chromium; not Ubuntu snap):

```bash
curl -fsSL -o /tmp/node20.tar.xz \
  https://nodejs.org/dist/v20.18.1/node-v20.18.1-linux-x64.tar.xz
(cd /tmp && npm pack playwright-core@1.49.1 && mv playwright-core-*.tgz playwright-pack.tgz)
./scripts/merion-bake-browser-agent.sh
```

Deploy with `template=browser-agent`, `browser_port=9222`, `confinement: strict`.
Driver listens on guest `:9230` (a11y refs). Operator:

```bash
merionctl browser tabs "$SESSION"
merionctl browser shot "$SESSION"
```

MCP tools: `browser_open` / `browser_snapshot` / `browser_act` / `browser_tabs` /
`browser_close`. Passwords: host `POST /v1/sessions/{id}/browser/fill-secret`.
Details: [docs/merion/browser/DRIVER.md](../merion/browser/DRIVER.md).

CI syntax check: `./scripts/merion-bake-browser-smoke.sh`.

---

## Appendix — Packaged agents (infra, migration, deploy)

Merion ships Fabric-facing packs under [`examples/merion-agents/`](../../examples/merion-agents/):

| Pack | Reads | Writes (ask) | Artifact |
|---|---|---|---|
| `infra-ops` | alerts, VMs, lifecycle compliance | restart / remediation | incident timeline |
| `migration-op` | `/api/migrations/*`, GuestKit inspect | create/cancel/rescue | wave plan + checklist |
| `deploy-op` | `/readyz`, `/health` | none by default | FABRIC_DOCTOR readiness |
| `browser-research` | allowlisted browse via a11y driver | none by default | research markdown |
| `pdf-brief` | local PDF only | none | `brief.md` (0 CONNECT) |

Shared `_fabric` connector + `fabric-api` credential recipe. Goals/artifacts API:
[`docs/merion/goals/README.md`](../merion/goals/README.md).

```bash
# Runtime must be up; credentials file includes fabric-api; FABRIC_API_TOKEN set
./scripts/merion-pack-demo.sh infra-ops
# Stub bundle without fabric-agent:
MERION_PACK_DRY=1 ./scripts/merion-pack-demo.sh deploy-op
# Stage demo (no browser):
./scripts/merion-demo-pdf.sh
```

Loop: create goal → session work → `POST /v1/artifacts` → advance `requires_approval` step →
`/v1/approvals` → audit. Cockpit exposes `active_goal` / `recent_artifacts`.

PDF demo deep-dive: [Tutorial 17](17-merion-pdf-brief.md) · [demos/pdf-brief.md](../merion/demos/pdf-brief.md).
