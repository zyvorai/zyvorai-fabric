# Production readiness (what landed vs what still needs hardware)

## Install Keep on a host (one command)

On a host that already runs FluxVM:

```bash
./scripts/deploy keep user@host            # Keep mode: signed deploys only
./scripts/deploy keep user@host --dev      # dev: unsigned packs allowed
./scripts/deploy keep user@host --dry-run  # print the plan, change nothing
```

It creates a signer seed on **your** machine (`~/.config/zyvor/keep-signer.seed`, never copied to
the host), deploys Fabric and the console, builds and installs agent-runtime with its systemd unit,
registers only your **public** key as a trusted signer, points fabricd at it, and runs the CSV cleanup
once as a smoke test (the PDF demos also need a template with `pdftotext`; it reports if yours has none). It does not install FluxVM or bake templates: if FluxVM is not answering, or
the demo template does not boot, it stops and says what to do. Re-running is safe.

Check a running install any time with `./scripts/merionctl doctor [--smoke]`.

## Install Keep on Kubernetes

`charts/zyvor-merion` runs the Keep runtime as a `hostNetwork` DaemonSet next to FluxVM on each node. It does not install
FluxVM: install the `zyvor-fabric` chart (or run FluxVM yourself) on the same nodes, which need `/dev/kvm`.

```bash
# 1. build and push the runtime image (agent-runtime/Dockerfile), then:
helm install keep charts/zyvor-merion -n keep --create-namespace \
  --set global.imageRegistry=registry.example.com \
  --set runtime.image.tag=0.3.0 \
  --set 'merion.trustedSigners={<64-hex Ed25519 public key>}'
```

- Keep mode is on by default and the chart refuses to render without a well-formed trusted signer. The signing seed
  never goes in values; only the public key does.
- The API token is generated once and kept across upgrades, or supplied with `security.existingSecret` (key
  `api-token`). The pod reads it from the Secret; it is not in the manifest.
- The API listens on the node's loopback. Reach it with `kubectl port-forward pod/<name> 9096:9096`. The egress
  broker and CONNECT proxy are bound on the node so cells can reach them; restrict them with the node firewall.
- Credentials: `credentials.descriptors` (no secrets in it), `runtime.extraVolumes` and `runtime.extraVolumeMounts` for files a credential `source`
  reads, and `serviceAccount.name` for Vault's Kubernetes login. See [vault/README.md](vault/README.md#on-kubernetes).
- State lives on the node under `runtime.stateHostPath`. Deleting the release does not delete it.
- The chart and the image have been rendered and schema-checked (`helm lint`, `kubeconform`); they have not been run
  in a cluster, and the image has not been built.

## Keep 0.1 pilot — release gate

Run on a customer-like FluxVM host with a registered agent template (`node22-agent` / `agent-node`; bake it with `./scripts/merion-bake-node22-agent.sh`):

```bash
MERION_E2E_TEMPLATE=node22-agent ./scripts/merion-pilot-gate.sh
# Archives under docs/merion/pilot-runs/<stamp>/{happy,deny}/
```

| Item | How to verify |
|---|---|
| Template required | Missing template → **FAIL** (no soft PASS) |
| Happy + deny | Gate runs twice; deny path shows no unapproved mutate |
| Signed Keep mutations | `ZYVOR_AGENT_MERION_MODE=1` + trusted signers; startup rejects empty signers. PUT policy needs `X-Merion-Policy-Signature`; POST agent needs `X-Merion-Manifest-Signature` over the exact JSON body. |
| Session / cockpit / restart | Cockpit `software-test`; session recovers after runtime restart |
| OOB approvals | Webhook + approve/deny outside agent chat |
| Console Keep view | `/app/merion/:sessionId` — goal, task, evidence, approval, outcome |
| infra-ops pack | `./scripts/merion-pack-demo.sh infra-ops` |

Latest archived run: [pilot-runs/20260924T182930Z](pilot-runs/20260924T182930Z/)
(**Firecracker** `node22-fc` / `flux-vm`, `guest_worker=ok`). Prior QEMU:
[20260924T141927Z](pilot-runs/20260924T141927Z/),
[20260924T154950Z](pilot-runs/20260924T154950Z/). Bake FC with
[`scripts/merion-bake-fc-rootfs.sh`](../../scripts/merion-bake-fc-rootfs.sh);
pilot gate prefers `node22-fc` when registered.

## Keep 0.1 — live proof (lab release gate)

| Item | How to verify |
|---|---|
| Stub Keep e2e (CI) | `./scripts/merion-e2e.sh` · `.github/workflows/merion.yml` |
| **Live FluxVM Keep proof** | `MERION_E2E_FLUXVM=1 ./scripts/merion-e2e.sh` or `./scripts/merion-live-lab.sh` (needs `MERION_E2E_TEMPLATE`) |
| Keep-mode signed policy | `ZYVOR_AGENT_MERION_MODE=1` + trusted signers; unsigned PUT → 403; unsigned deploy → 403; start refuses empty signers |
| Credential authority | `authorize_resolve` allowlists host/method/path/user; secrets still host-env (not 0.2 unwrap) |
| Confine | `ZYVOR_AGENT_CONFINE=1` / `confinement: strict` on live path |
| Measured profile | `ZYVOR_AGENT_SECURITY_PROFILE=measured` (Keep mode default); cockpit `evidence_class: software-test` |
| Browser live view | `GET /v1/sessions/{id}/browser/view` + `/browser/screenshot` (JPEG via CDP bridge); no input takeover |
| Cockpit / approvals | `GET /merion/cockpit?session=` · out-of-band webhook |

```bash
# CI default — FluxVM stub
./scripts/merion-e2e.sh

# Lab release gate — live FluxVM (template required)
MERION_E2E_TEMPLATE=node22-agent ./scripts/merion-live-lab.sh
```

To deploy in enforced Keep mode, sign the exact JSON file you send:

```bash
merionctl policy sign agent.json   # uses MERION_POLICY_SEED; writes agent.json.sig
merionctl create -f agent.json --signature agent.json.sig
```

The signer public key belongs in `ZYVOR_AGENT_POLICY_TRUSTED_SIGNERS` on the
runtime. A policy update still signs the exact YAML sent to PUT. Generic agent
runtime installations can leave `ZYVOR_AGENT_MERION_MODE` unset. Existing agents
created before enforced mode must be redeployed with a signed manifest before
being treated as policy-verified; signing does not retroactively attest them.

## Landed in this tree (software / control plane)

| Item | How to verify |
|---|---|
| Signed Keep mutations | Keep mode or `ZYVOR_AGENT_POLICY_TRUSTED_SIGNERS`; PUT policy + POST deploy require matching Ed25519 signatures (`X-Merion-Policy-Signature` / `X-Merion-Manifest-Signature`) |
| Export-token gate | `POST /v1/export-tokens`; pack/export need `X-Merion-Export-Token` |
| Firecracker cell | FluxVM Firecracker templates; `cell_backend: firecracker` |
| Full pack metadata | `merionctl pack` → policy, agent pin, vault **names** (no raw secrets) |
| Phone approvals | Webhook `ui.actions` + `channel: out_of_band` |
| Runtime control-plane e2e | `agent-runtime/scripts/e2e-no-fluxvm.sh` |
| Goals / artifacts / packs | `/v1/goals`, `/v1/artifacts`; [`examples/merion-agents/`](../../examples/merion-agents/); [`docs/merion/goals/README.md`](goals/README.md) |

## Still not a TEE claim (Keep 0.2 + hardware)

- Flipping `security.snp_launch_verified` / `tdx_launch_verified` on FluxVM
- User-held unwrap (phone/YubiKey) before vault open
- Real SNP/TDX launch + block-device home (see confidential-agent-vms.md)

Software scaffolding already in tree (still `software-test`):

- Cockpit / console **attestation receipt** (never labels software-test as unread)
- `POST /v1/sessions/{id}/host-recover` — **forbidden** on confidential; measured
  needs dual recover keys (`ZYVOR_AGENT_RECOVER_KEY_A` / `_B`)
- Browser **screenshot** + **screencast** (frames only; no input) — fabricd
  `WS /ws/sessions/{id}/browser/screencast?token=`
- User-held **challenge** API (`merionctl user-held-*`) — complete refused without SNP/TDX;
  fabricd `/api/vault/user-held/*` + `/api/vault/status`
- `scripts/merion-bake-browser-agent.sh` for Chromium template bake

Until then: **the host can still see a measured VM.** That is intentional honesty, not an unfinished checkbox we can close in CI without silicon.
