# Fabric AI Workloads (Beta on a single cluster)

OpenAI-compatible inference on dedicated NVIDIA GPU VMs, operated through
Fabric’s API, CLI, and console. Maglev load-balances backends with
control-plane weight updates. Phases 0–6 cover inventory, routing, autoscaling,
API keys, sites, and Agent Runtime. Preview 2 / Beta adds revisioned rollouts, model
supply-chain jobs, gateway limits, a per-process Raft lease, live `nvidia-smi`
readings, site-to-site model copy, a WebSocket inference proxy, optional
OTLP export, PCI MIG behind `FLUXVM_AI_PCI_MIG`, and known runtimes enabled by
default. **Single-cluster AI Workloads are Beta.** Multi-site HA store stays Preview.
Do not treat this as GA.

## How to use (start here)

Step-by-step operator walkthrough (Janus lab GPU **or** real NVIDIA):

- **In-repo tutorial:** [tutorials/15-ai-workloads.md](tutorials/15-ai-workloads.md)
- **Website tutorial:** https://zyvor.dev/docs/zyvor-fabric-manual/ai-workloads
- **Blog intro:** https://zyvor.dev/blog/fabric-ai-workloads-tutorial
- **Lab smoke:** `FABRIC_URL=https://127.0.0.1:9095 ./scripts/smoke-ai-janus-lab.sh`

### Shortest path (CLI)

```bash
export FABRIC_URL=https://127.0.0.1:9095 ZYVOR_FABRIC_URL=$FABRIC_URL
# Obtain ZYVOR_FABRIC_TOKEN via POST /api/auth/login (admin password file)

fabricctl ai gpus
fabricctl ai node list
fabricctl ai model add demo-qwen --source hf://Qwen/Qwen3-8B
fabricctl ai profile add demo-24g --runtime vllm --gpu 1 --vram 24 --cpu 8 --memory 32
fabricctl ai deploy demo-qwen --profile demo-24g --replicas 1
# Leave preferred_site unset when only Janus GPUs exist (site=janus)
fabricctl ai endpoint expose demo-qwen --openai-compatible
fabricctl ai key create demo-key --endpoint demo-qwen-openai
# Then: POST $FABRIC_URL/api/ai/openai/demo-qwen-openai/v1/chat/completions
```

### Console

Open `/app/ai` after sign-in: **Models**, **Deployments**, **Endpoints**, **API keys**, **Nodes** (Janus inventory and MIG fields).

### Runtimes and MIG

| Topic | Behavior |
|---|---|
| Runtimes | `vllm`, `tensorrt-llm`, `triton`, `llama.cpp`, `tei` launch by default; `FLUXVM_AI_DENY_RUNTIMES` blocks; `FLUXVM_AI_ALLOW_RUNTIMES` is a strict allowlist when set |
| Janus MIG | Record-only slices via `fabricctl ai node mig-create` (no `nvidia-smi`) |
| PCI MIG | Requires `FLUXVM_AI_PCI_MIG=1`; optional `FLUXVM_AI_PCI_MIG_RECORD_ONLY=1` without driver |
| Raft | `FLUXVM_AI_RAFT_ID` / `PEERS` / `TOKEN` — three processes; rate counters and audit tip on the leader |

Trace capture, the decision router and A/B reports are described in
[ai-traces-and-routing.md](ai-traces-and-routing.md).

## Maturity

| Capability | Status |
|---|---|
| Service Fabric Maglev | GA |
| Fabric AI Workloads (single cluster) | **Beta** |
| Revisioned rollouts, model jobs, gateway limits, Raft lease | **Beta** |
| Multi-node GPU scheduler | **Beta** |
| Multi-site federation HA store, explain/policy cross-site | **Preview** |
| In-tree `flux-vm` multi-vCPU | Experimental until `scripts/test-kvm-smp-boot.sh` is green on lab hardware |
| Machina local LLM | Separate roadmap product — not Fabric |

## First release shape

- One to eight NVIDIA GPUs per QEMU VM, reserved together and passed as VFIO devices. `gpu.count` of 1 keeps the previous single-device path.
- Runtime: **vLLM**, TensorRT-LLM, Triton, llama.cpp, and text-embeddings-inference
  (`tei`) are known and launch by default. Set `FLUXVM_AI_DENY_RUNTIMES` to block a
  name. When `FLUXVM_AI_ALLOW_RUNTIMES` is set, it is a strict allowlist. The guest
  image must still contain the binary.
- Model source: `hf://org/name` or a pre-staged host path. A model job passes the stored Hugging Face `revision` to the downloader and keeps bytes under `{state}/ai-models` by digest. Checksums for a directory are a sorted manifest (relative path, size, SHA-256), not the first file. When `signature` is set, the job checks an HMAC-SHA256 of that digest with `FLUXVM_AI_MODEL_SIGNING_KEY`.
- Endpoint: Maglev VIP → guest `:8000` (`/v1/chat/completions`, `/v1/completions`, `/v1/embeddings`, `/v1/rerank`, `/v1/batches`)
- Readiness: Fabric HTTP `GET /health` before Maglev `Ready`
- Manual + automatic replica scaling; drain on delete / maintenance
- Bridged tap NICs (Maglev does not attach to bridge-less direct taps)

## Resources

| Resource | Purpose | Janus / Zynera cousin |
|---|---|---|
| `ModelArtifact` | Model identity + host cache path + residency/license | `FabricAIJob.spec.model` |
| `InferenceProfile` | Runtime + GPU/CPU/network shape | job GPU request + calibrated profile |
| `InferenceDeployment` | Desired replicas + autoscaling + rollout | `FabricAIJob` with `type: inference` |
| `InferenceEndpoint` | OpenAI Maglev frontend + site/residency | Service / VIP |
| `InferenceApiKey` | Scoped endpoint keys (hashed at rest) | — |
| `GET /api/ai/gpus` | Inventory + allocation | `FabricGpuNode` |
| Quota `max_gpus` | Tenant GPU cap | `FabricQuota.spec.gpuQuota.maxGPUs` |

[Zyvor Janus](https://github.com/hypersdk/zyvor-janus) simulates these shapes
without physical GPUs. Use Janus for scheduling policy experiments; use Fabric
for the live dataplane.

## REST

```text
GET/POST   /api/ai/models
GET/DELETE /api/ai/models/{name}
GET/POST   /api/ai/profiles
GET/DELETE /api/ai/profiles/{name}
GET/POST   /api/ai/deployments
GET/DELETE /api/ai/deployments/{name}
POST       /api/ai/deployments/{name}/scale
PUT        /api/ai/deployments/{name}/autoscaling
POST       /api/ai/deployments/{name}/drain
POST       /api/ai/deployments/{name}/rollout
POST       /api/ai/deployments/{name}/rollouts
POST       /api/ai/deployments/{name}/rollouts/{id}/pause|resume|promote|rollback
GET/POST   /api/ai/deployments/{name}/revisions
GET        /api/ai/deployments/{name}/revisions/{revision}
GET        /api/ai/deployments/{name}/metrics
GET/POST   /api/ai/endpoints
GET/DELETE /api/ai/endpoints/{name}
GET/POST   /api/ai/keys
DELETE     /api/ai/keys/{id}
GET        /api/ai/gpus
GET/POST   /api/ai/nodes
GET        /api/ai/nodes/{id}
POST       /api/ai/nodes/{id}/heartbeat
POST       /api/ai/nodes/{id}/gpus/{bdf}/mig
POST       /api/ai/nodes/{id}/mig
DELETE     /api/ai/nodes/{id}/mig/{bdf}
POST       /api/ai/admit
POST       /api/ai/admit/{token}
GET/POST   /api/ai/sites
GET        /api/ai/sites/{id}
GET        /api/ai/explain/placement/{deployment}
GET        /api/ai/explain/scaling/{deployment}
GET        /api/ai/explain/routing/{endpoint}
GET        /api/ai/explain/failure/{replica}
POST       /api/ai/policies
POST       /api/ai/backup
POST       /api/ai/restore
GET        /api/ai/capacity
GET        /api/ai/events
GET        /api/ai/model-jobs
POST       /api/ai/models/{name}/materialize
GET        /api/ai/models/{name}/status
POST       /api/ai/models/{name}/verify
POST       /api/ai/models/cache/evict
ANY        /api/ai/openai/{endpoint}[/{path}]   # API-key or fabric-inference JWT (no control-plane JWT)
```

## CLI

```bash
fabricctl ai model add qwen3-8b --source hf://Qwen/Qwen3-8B
fabricctl ai profile add edge-24g --gpu 1 --vram 24 --cpu 8 --memory 32
fabricctl ai deploy qwen3-8b --profile edge-24g --replicas 2
fabricctl ai endpoint expose qwen3-8b --openai-compatible --routing least_queue
fabricctl ai deployment scale qwen3-8b --replicas 4
fabricctl ai deployment autoscale qwen3-8b --enable --min 1 --max 4
fabricctl ai deployment drain qwen3-8b
fabricctl ai deployment rollout qwen3-8b --strategy canary --canary-percent 10
fabricctl ai key create edge-key --endpoint qwen3-8b-openai
fabricctl ai capacity
fabricctl ai gpus
fabricctl ai node list
fabricctl ai node mig-create node-0 --parent-bdf janus:node-0:gpu-0 --profile 1g.10gb
fabricctl ai node mig-delete node-0 janus:node-0:gpu-0--1g.10gb--0
```

## OpenAI gateway

Point clients at Fabric (not the Maglev VIP directly) when you need API-key
auth, an inference OIDC token, and request quotas:

```bash
export OPENAI_BASE_URL=https://fabric.example:9095/api/ai/openai/qwen3-8b-openai
export OPENAI_API_KEY=fvai_…   # from fabricctl ai key create
# or: Authorization: Bearer <OIDC JWT with aud=fabric-inference>
curl -sk "$OPENAI_BASE_URL/v1/chat/completions" \
  -H "Authorization: Bearer $OPENAI_API_KEY" \
  -H 'content-type: application/json' \
  -d '{"model":"qwen3-8b","messages":[{"role":"user","content":"hi"}]}'
```

The gateway validates a scoped API key (prefix lookup plus a constant-time HMAC
compare, endpoint scope, and optional model scope) or an OIDC JWT whose issuer
matches an enabled provider and whose audience is exactly `fabric-inference`.
A control-plane JWT with any other audience is rejected unless
`FLUXVM_AI_GATEWAY_ACCEPT_JWT=1`. After auth it reserves one lifetime request
under a per-key lock (OIDC callers skip that persist), then proxies to the Maglev
VIP or a ready replica. The upstream
body is streamed (`text/event-stream` and other non-hop headers are preserved).
There is a total upstream deadline of 120 seconds, overridable with
`x-request-timeout` from 1 to 300 seconds. The idle timeout between chunks
defaults to 60 seconds (`FLUXVM_AI_GATEWAY_IDLE_SECS`). Set
`FLUXVM_AI_SHED_QUEUE` to refuse a request when any ready replica's queue
depth is above that value; unset or `0` does not shed. `x-request-priority`
is `low`, `normal`, or `high`. Low sheds at half that queue and high at double.
`FLUXVM_AI_DAILY_TOKENS` is a separate 24-hour token bucket for the whole gateway.
`FLUXVM_AI_MAX_PROMPT_TOKENS` and `FLUXVM_AI_MAX_GENERATED_TOKENS` refuse a call
whose prompt estimate or `max_tokens` is above that cap. Unset or `0` leaves
the cap off. `FLUXVM_AI_GLOBAL_RPM`, `FLUXVM_AI_TENANT_RPM`, `FLUXVM_AI_PROJECT_RPM`,
`FLUXVM_AI_MODEL_RPM`, and `FLUXVM_AI_USER_RPM` share one stored one-minute
window with `FLUXVM_AI_GATEWAY_RPM`. The matching `*_TPM` variables count tokens
in that same window. `FLUXVM_AI_GLOBAL_STREAMS`, `FLUXVM_AI_TENANT_STREAMS`,
`FLUXVM_AI_PROJECT_STREAMS`, `FLUXVM_AI_GATEWAY_STREAMS`, `FLUXVM_AI_MODEL_STREAMS`,
and `FLUXVM_AI_USER_STREAMS` cap in-flight calls on those same scopes. The count
drops when the response stream ends. A scope is checked when any of its caps is set.
The tenant window applies only when the API
key has a tenant. The project window applies only when `x-project-id` is set.
The user window applies only when `x-user-id` is set. `x-session-id`, or the first 64 characters of the prompt when that header is
absent, selects one replica. That replica is placed immediately after the VIP,
or first when there is no VIP. The prefix is not written to the audit log. A connect failure is
tried once on another ready replica, and only when the request is not streaming
and no response byte has arrived. Upstream stays `http://` unless
`FLUXVM_AI_BACKEND_TLS=1`. With that flag and no client files, the gateway uses
HTTPS and the process trust store. mTLS needs all three of
`FLUXVM_AI_BACKEND_CLIENT_CERT`, `FLUXVM_AI_BACKEND_CLIENT_KEY`, and
`FLUXVM_AI_BACKEND_CA`. A partial set is refused. When the chosen upstream is the
configured Janus hostport, the gateway also sends
`Authorization: Bearer $FLUXVM_AI_JANUS_API_KEY` if that env is set. The body is also limited to
`FLUXVM_AI_GATEWAY_MAX_BODY` bytes (default 1 MiB). Context is `max_tokens`
plus about one token per four prompt characters, and it must fit in
`FLUXVM_AI_MAX_CONTEXT` (default 32768) and any tighter tenant
`max_context_tokens`. The prompt is not stored. Dropping the client cancels the
upstream request. An audit row is written as `ATTEMPT`, then `SUCCESS` or
`FAILED`. Maglev itself remains L4 and does not see API keys.

Under `FLUXVM_AI_DRY_RUN=1` the gateway returns a synthetic chat completion
after accepting the key. A JSON body with `"stream": true` returns two SSE
chunks instead of one buffered completion.

## Phase map

| Phase | What shipped |
|---|---|
| 0–1 | GPU inventory/bind, Model/Profile/Deployment/Endpoint, Maglev equal weight, CLI/console |
| 2 | AI-aware Maglev weights from vLLM `/metrics` (`least_queue`, `lowest_ttft`, …) |
| 3 | Queue/TTFT autoscaler, drain, rolling/canary/blue-green rollout + rollback thresholds |
| 4 | Endpoint API keys, checksum enforcement, residency/license on models, revision audit |
| 5 | `site_local` / `cost_optimized` / `energy_optimized` routing, preferred/allowed sites |
| 6 | Agent Runtime `kind: fabric` credentials + `ZYVOR_FABRIC_INFERENCE_BASE` shim |
| Harden | OpenAI API-key gateway, `/ai/capacity` + `/ai/events`, console keys/autoscale, golden-image bake script |
| GitOps | Terraform `zyvor-fabricd_{model_artifact,inference_profile,inference_deployment,inference_endpoint,ai_site}` plus operator CRDs. The operator watches each installed AI kind and skips a missing CRD. Installed kinds post to the existing fabricd routes. |
| Correctness | Exclusive GPU/VIP create, health replacement after three failures, Maglev-before-VIP delete, CIDR network/broadcast skipped, bounded reconcile retry. Still **Preview** |

## Terraform

```hcl
resource "zyvor-fabricd_model_artifact" "qwen" {
  name   = "qwen3-8b"
  source = "hf://Qwen/Qwen3-8B"
  format = "safetensors"
}

resource "zyvor-fabricd_inference_profile" "edge" {
  name             = "edge-24g"
  minimum_vram_gib = 24
  cpu              = 8
  memory_gib       = 32
}

resource "zyvor-fabricd_inference_deployment" "qwen" {
  name     = "qwen3-8b"
  model    = zyvor-fabricd_model_artifact.qwen.name
  profile  = zyvor-fabricd_inference_profile.edge.name
  replicas = 2
}

resource "zyvor-fabricd_inference_endpoint" "qwen" {
  name       = "qwen3-8b-openai"
  deployment = zyvor-fabricd_inference_deployment.qwen.name
}

resource "zyvor-fabricd_ai_site" "pune" {
  id        = "pune-1"
  residency = "india"
}
```

See `terraform-provider/examples/ai-workloads/`.

## Kubernetes operator

```bash
kubectl apply -f operator/examples/ai-inference-deployment.yaml
# The operator reconciles ModelArtifact and InferenceDeployment.
# It also lists InferenceProfile, InferenceEndpoint, InferenceRollout,
# GpuNode, AiSite, InferenceApiKeyPolicy, and InferenceAutoscaler.
# Those extra kinds are not watched.
```

## AI-aware Maglev routing (Phase 2+)

Fabric periodically scrapes each ready replica’s vLLM Prometheus
`/metrics` and updates Maglev backend weights (1–32). The eBPF program
never sees models or tokens — only weights.

| Strategy | Behaviour |
|---|---|
| `equal` | Weight 1 for every ready backend |
| `least_queue` | Higher weight when `num_requests_waiting` is low |
| `lowest_ttft` | Higher weight when observed TTFT is low |
| `most_free_vram` | Higher weight when GPU/KV-cache usage is low |
| `weighted_capacity` | Average of queue + free-VRAM scores |
| `site_local` | Prefer replicas tagged with the endpoint preferred site |
| `cost_optimized` | Prefer lower `cost_tier` replicas |
| `energy_optimized` | Prefer lower GPU-cache pressure |
| `highest_throughput` | Higher weight when observed tokens per second are higher |
| `lowest_failure` | Higher weight when the share of aborted or errored requests is lower |

A replica is eligible for Maglev only when it is ready, not draining, and its
last scrape is younger than 30 seconds. A failed scrape or a missing
`scraped_at` is omitted. Zeros from a failed scrape are not treated as idle.

Background tasks: `ai_routing_controller` (10s), `ai_autoscaler` (15s),
`ai_reconcile_controller` (15s). Under `FLUXVM_AI_DRY_RUN=1` synthetic metrics
still diverge Maglev weights.

Endpoint VIPs are allocated from a pool that uses Rivora AddressPool syntax
(CIDR, `start-end` range, or a single IPv4 address). Set
`FLUXVM_AI_ADDRESS_POOL` (comma-separated) or `FLUXVM_AI_SERVICE_CIDR`
(default `10.96.0.0/16`). `FLUXVM_AI_AVOID_BUGGY_IPS` defaults on and skips
addresses whose last octet is 0 or 255. For a prefix of /30 or wider, the
network and broadcast addresses are also skipped (`10.96.0.0/30` yields only
`.1` and `.2`). Each address is inserted with `create_new`, so two fabricd
processes cannot take the same VIP. The address is released when Maglev is
absent, including a delete that returns 404. If Maglev is still present, or
FluxVM cannot be reached, the endpoint stays `Deleting` and the VIP is kept. Prefixes larger than /16, and IPv6, stay on the
Rivora controller.

## Autoscaling and rollouts (Preview 2)

Creating a deployment writes revision 1 and one replica set. `status.replicas` stays the union of every replica set, so the gateway and console keep reading one list. A rollout is a stored record (`strategy`, `from_revision`, `to_revision`, `max_surge`, `max_unavailable`, `phase`) advanced by the 15-second reconciler under the deployment lease. A restart continues that phase instead of starting over.

Rolling update adds one new-revision replica, bounded by `max_surge`, waits until it has been ready for the minimum healthy duration, gives it Maglev weight, then drains one old replica, bounded by `max_unavailable`. A failed new replica does not remove the last ready replica of the previous revision. When every replica is on the new revision, the old revision is `Superseded`.

Canary uses a separate replica set and the steps 5% / 300s, 20% / 600s, 50% / 900s, and 100%. Maglev weight is the step weight on the new revision versus the old one. The rollout pauses when the mean error rate or time-to-first-token exceeds the rollback thresholds. `pause`, `resume`, `promote`, and `rollback` continue, finish, or restore the previous revision. Rollback sets the previous revision back to `Active`, restores its Maglev weights, and stops the new set. It does not delete the last healthy old replica.

Blue/green builds the full new replica set, then switches weights in one save. The old set stays until the rollback window ends.

Maturity for a **single cluster** is **Beta**. The control plane records revisioned rollouts, content-addressed model jobs (including format scanning and derived optimization artifacts), gateway limits, optional backend TLS, batch job records, reported GPU temperature and ECC filters, multi-GPU reservation, site policy, chargeback, and chaos recovery classes. Known runtimes launch by default; `FLUXVM_AI_DENY_RUNTIMES` blocks a name and `FLUXVM_AI_ALLOW_RUNTIMES` is an optional strict allowlist. `FLUXVM_AI_CONSENSUS=external` is only a label. Multi-site HA store stays **Preview**. This is not GA.

Each `zyvor-fabricd` process is one Raft voter when `FLUXVM_AI_RAFT_ID`, `FLUXVM_AI_RAFT_PEERS` (at least three `id@host:port` entries, including this process), and `FLUXVM_AI_RAFT_TOKEN` are set. The log is `{state}/ai-raft/`. RequestVote and AppendEntries use `POST /api/ai/raft/{vote,append,snapshot}` with `x-raft-token`. The replicated record is the current leader id. AI placement runs only while this process is the committed leader of that membership. With those variables unset, placement stays as it is on a single process. This process never starts the other voters. A missing membership, or fewer than three committed voters, leaves leader-loss recovery degraded.

On Linux, a node heartbeat runs `nvidia-smi` when that binary is on `PATH` and writes `temperature.gpu`, `power.draw`, and `ecc.errors.uncorrected.volatile` onto a matching PCI BDF. A missing binary, a failed command, or a `janus:` id leaves the stored fields unchanged. `POST /api/ai/nodes/{id}/mig` creates Janus slice records without the driver. A PCI parent BDF requires `FLUXVM_AI_PCI_MIG=1` and runs `nvidia-smi mig` (or inventory-only when `FLUXVM_AI_PCI_MIG_RECORD_ONLY=1`). Without that flag, a PCI MIG create returns 400.

`AiSite.peer_url` is optional. `POST /api/ai/models/{name}/replicate` stays `pending` unless a node at the site already cached the model, or the peer returns 201 from `PUT /api/ai/models/{name}/blobs/{digest}` after the SHA-256 matches. The receiver writes only under its model directory. A missing `peer_url`, a missing file, a hash mismatch, or a wrong `FLUXVM_AI_REPLICATE_TOKEN` does not mark the record `ready`.

An inference request with `Upgrade: websocket` is authenticated on the same gateway path, then bridged to the `ws` or `wss` form of the selected upstream. The upstream is contacted before the client upgrade completes, so a refused upstream is 502. mTLS uses the same client certificate and CA as HTTP. The stream slot is held until the socket closes. A request without the upgrade stays on the HTTP proxy.

When `FLUXVM_AI_OTEL_ENDPOINT` is set, each gateway call exports one OTLP/HTTP span to `{endpoint}/v1/traces` with the endpoint name, HTTP status, and token count. The prompt and the API key are not attributes. Export failure is logged and does not fail the request. Unset leaves the existing tracing subscriber unchanged.

The operator discovers each AI CRD before watching it. A missing kind is logged and skipped, and startup continues. Installed kinds post to the existing fabricd routes: profiles, endpoints, deployment rollouts, nodes, sites, and deployment autoscaling. `InferenceApiKeyPolicy` is stored with `PUT /api/ai/key-policies/{endpoint}` and caps `ttl_secs` on key create. A CRD that is not installed does nothing.

## Multi-node scheduler (Phase 10)

Register hosts with `POST /api/ai/nodes`. Each node reports site, failure domain, GPUs, free CPU and memory, cached models, and taints. `POST /api/ai/nodes/{id}/heartbeat` refreshes it. A heartbeat older than 60 seconds marks the node offline for scheduling.

Placement is filter then score: ready state, no taints, residency, free NVIDIA VRAM, then cache hit, preferred site, and failure-domain spread. The same inputs pick the same node. When no nodes are registered, replicas still land on the local FluxVM inventory. A chosen GPU that this FluxVM process does not have is not started here. A replica on an offline node is replaced only when another ready node exists, so the last healthy replica is kept when nothing else can take it.

When `FLUXVM_AI_JANUS_URL` is set and FluxVM reports no NVIDIA GPU, Fabric reads `GET /api/cluster?config=single_gpu` and stores the device as `janus:node-0:gpu-0` with `source` carried in the model name `janus:…`. A Matrox or other non-NVIDIA display adapter does not count as an inference GPU. The replica address is the Janus host and port. No VFIO bind and no VM are created. The gateway proxies a ready replica at that address even when `FLUXVM_AI_DRY_RUN=1`. Set `FLUXVM_AI_JANUS_API_KEY` to the Janus shim bearer so that hop authenticates. That device is the Janus scheduler simulator, not a PCI NVIDIA GPU. The Janus node’s site is `janus`; leave `preferred_site` unset (or set it to `janus`) so placement can select that node — a preferred site of `lab` alone rejects it as “no free matching GPU” when the only free device is under the Janus site.

`POST /api/ai/nodes/{id}/mig` creates a slice record for a Janus parent when the body includes `parent_bdf` (for example `janus:node-0:gpu-0`) and the profile is in the H100 catalog (`1g.10gb`, `1g`, `2g.20gb`, `2g`, `3g.40gb`, `3g`, `7g.80gb`, `7g`) and the memory still fits. The same catalog applies to a PCI parent when `FLUXVM_AI_PCI_MIG=1`; Fabric then runs `nvidia-smi mig -cgi … -C` unless `FLUXVM_AI_PCI_MIG_RECORD_ONLY=1`. `DELETE /api/ai/nodes/{id}/mig/{bdf}` removes that slice when no replica holds it (and destroys PCI instances when PCI MIG is enabled). Placement uses the slice and leaves the parent unschedulable while slices exist. A heartbeat on Linux fills `temperature_c`, `power_watts`, and `ecc_errors` for a matching PCI BDF when `nvidia-smi` is on `PATH`. `0` still means unknown. A missing binary, a failed command, or a `janus:` id does not invent those readings and does not clear a value the node already reported. `FLUXVM_AI_MAX_GPU_TEMP_C` skips a GPU hotter than that value. `FLUXVM_AI_REJECT_ECC=1` skips a GPU whose ECC count is above zero.

When a ready replica address is an IP hostport such as `127.0.0.1:30818`, Maglev stores the IP and that port. A hostname upstream is omitted from Maglev; the OpenAI gateway still proxies it. An endpoint whose replicas have no IP backends, or whose ready replicas are all Janus or `dry-run-*` devices, reserves a VIP and skips the Maglev upsert so the reconciler does not log a FluxVM error every tick.

`POST /api/ai/admit` runs the tenant policy for a JSON body or an `AdmissionReview`. `POST /api/ai/admit/{token}` is the same check when `FLUXVM_AI_ADMIT_TOKEN` is set. The operator chart renders a validating webhook for `InferenceDeployment` only when `admissionWebhook.enabled=true`. An unreachable fabricd fails closed. On the lab k3s host, point `admissionWebhook.fabricdUrl` at the host HTTPS listen (for example `https://127.0.0.1:9095` from node-local apiserver paths, or the node address) and set `caBundle` to the base64 of fabricd’s TLS certificate so the apiserver trusts the hop.

The inference gateway still accepts scoped API keys. It also accepts an OIDC JWT when the issuer matches an enabled provider and the audience is `fabric-inference`. `sub` becomes the tenant unless the token has a `tenant` claim. A control-plane JWT with any other audience is rejected. Maturity stays **Preview**.

## Sites, policy, and the rest of the roadmap (Phases 11–17)

`POST /api/ai/sites` records a site. Routing spills from a saturated preferred site only to `failover_sites`, and never to a site outside `residency`. `minimum_sites` and `max_replicas_per_site` are enforced by the scheduler. A disconnected site may keep serving a snapshot until `fail_closed_unix` (`edge_may_serve`).

`GET /api/ai/explain/placement/{deployment}` returns the chosen node and why the others were rejected. `GET /api/ai/explain/scaling/{deployment}` reports the instantaneous scale-out, scale-in, or hold signal; the autoscaler still waits its configured seconds before changing the replica count. `GET /api/ai/explain/routing/{endpoint}` says why each replica is included or excluded. `GET /api/ai/explain/failure/{replica}` lists recorded reasons for one replica id or VM name. `POST /api/ai/policies` is a tenant allow-list for model name and source prefix. Optional `deploy_hour_start` and `deploy_hour_end` are UTC hours; both absent means deploys are always allowed, and a start after the end wraps past midnight. No policy means the existing tenant filter still applies. `POST /api/ai/backup` and `POST /api/ai/restore` copy desired-state JSON, not model bytes.

Chargeback is `GET /api/ai/finops`: GPU-seconds, reserved versus ready GPUs, and the current token window. `alert` is true when those tokens exceed `FLUXVM_AI_TOKEN_BUDGET`; unset or `0` does not alert. Each AI audit row extends a SHA-256 chain in the state store; when Raft peers are set, the tip also replicates as `LeaseCommand::AuditLink` on the leader. Known runtimes launch by default. Set `FLUXVM_AI_DENY_RUNTIMES` to block a name, or set `FLUXVM_AI_ALLOW_RUNTIMES` for a strict allowlist. There is no quorum inside one process. Set `FLUXVM_AI_RAFT_ID`, `FLUXVM_AI_RAFT_PEERS`, and `FLUXVM_AI_RAFT_TOKEN` on each voter when a three-process lease is required; rate counters and the audit tip then live on the leader. `FLUXVM_AI_CONSENSUS=external` does not create that lease. OIDC remains the control-plane login. The inference gateway accepts scoped API keys and an OIDC JWT whose audience is `fabric-inference`. Any other JWT audience is rejected unless `FLUXVM_AI_GATEWAY_ACCEPT_JWT=1`. Single-cluster maturity is **Beta**; multi-site HA store stays **Preview**.

```text
Scale out when:
  queue_depth > scale_out_queue for scale_out_seconds
  OR mean TTFT > scale_out_ttft_ms
  AND replicas < max_replicas

Scale in when:
  queue_depth < scale_in_queue for scale_in_seconds
  AND replicas > min_replicas (or 0 if scale_to_zero)
  AND every ready replica has fresh metrics
```

Scale-out uses the same quota check as a manual scale. If that check fails, or
if scale-in is skipped because metrics are missing or stale, the decision is
recorded on `deployment.status.message` and the replica count is left unchanged.

```bash
fabricctl ai deployment autoscale qwen3-8b --enable --min 1 --max 4 \
  --scale-out-queue 20 --scale-out-seconds 30
fabricctl ai deployment drain qwen3-8b --grace-seconds 30
fabricctl ai deployment rollout qwen3-8b --strategy canary --canary-percent 10
```

## Reconciliation

`ai_reconcile_controller` runs every 15 seconds and reconciles up to four
deployments at once. Create, scale, the autoscaler, and that loop take a
per-deployment lock, then a file lease (owner, fencing token, expiry) on the
state directory. That lease is not an etcd lock: two fabricd processes on
different hosts still need a shared transactional store. Each replica stores
`replica_id` and `ordinal`. A new replica takes the first unused ordinal, not
`replicas.len()`, so replacing one replica cannot collide with another.
Restart reloads that identity. A GPU or VIP file is
created with `create_new` before use. An empty or partial file is kept until
FluxVM shows the GPU or VIP is idle; if the device is still active the file is
rewritten, not deleted. A reservation that is not yet attached expires; once the VM or
endpoint exists it is pinned. The reservation is removed only after a
successful `release_host_gpu` (or a bind that never happened). A disappeared
VM is released the same way. Each tick probes `GET /health`, including replicas
that are already ready. Three consecutive failures clear ready, release the
GPU, delete the VM, and let scale-up replace it. Endpoint delete releases the
VIP when Maglev is absent, including a DELETE that returns 404. FluxVM
connection and timeout errors, and a lease held by another process, leave the
deployment `Pending`. A missing profile or model is `Failed`. Startup is the
first tick. The sweep removes reservations and Fabric-managed VMs whose
deployment is gone, and Maglev services that Fabric recorded for an endpoint
that no longer exists. It does not delete unrelated FluxVM services. Maturity
stays **Preview**.

`POST /api/ai/batches` records a job for a deployment. The body is
`deployment` and `input_sha256` (64 hex characters). The prompt is not stored.
The job starts `queued`. `POST /api/ai/batches/{id}/claim` moves it to
`running` only when that deployment has a ready replica that is not draining.
`POST /api/ai/batches/{id}/finish` with `{ "ok": true|false }` records success
or failure. Fabric does not mark a job succeeded by itself.
`POST /api/ai/batches/{id}/cancel` refuses a job that is already finished.

## Security (Phase 4)

- Per-tenant model / endpoint scoping (existing RBAC + tenant filters)
- `POST /api/ai/keys` — HMAC-SHA256 with `FLUXVM_AI_KEY_HMAC_SECRET` (a preview default is used when unset). `FLUXVM_AI_KEY_HMAC_SECRET_FILE` overrides that variable; a missing or empty file is an error and does not fall back to the preview secret. Plaintext is returned once. `GET /api/ai/keys` does not include `secret_hash`. `ttl_secs` sets `not_after_unix`; absent or `0` does not expire. `POST /api/ai/keys/{id}/rotate` issues a second secret and keeps the current one valid for `overlap_secs` (default 3600). This is not a certificate authority.
- Lifetime `request_quota`, plus optional per-key `tokens_per_minute` and `max_concurrent`. The gateway updates the key file under the same exclusive lock as the lifetime quota. In-flight count drops when the client cancels or the upstream stream ends. If the upstream usage object is missing, the request's `max_tokens` is the token count (or 1). Distributed counters across nodes are the Raft log when `FLUXVM_AI_RAFT_PEERS` is set: the leader applies the admit, followers forward to that leader, and a missing leader fails closed instead of keeping a second count. Unset peers keep the local file counter.
- `require_checksum` on ModelArtifact rejects unverified materialization. A directory checksum is the SHA-256 of a sorted manifest (`relative-path size sha256`), not the first file. `POST /api/ai/models/{name}/verify` repeats that check and refuses `.pkl`, `.pickle`, `.pt`, and `.pth` files. A `.safetensors` file is counted and preferred; a `.bin` file is not treated as a pickle. A set `signature` must be the HMAC-SHA256 hex of the digest under `FLUXVM_AI_MODEL_SIGNING_KEY`. An artifact with no signature is not signed.
- Model paths are canonicalized and must stay under `FLUXVM_AI_MODEL_DIR` or `{state}/ai-models`
- `license` + `residency` metadata on models; residency copied to deployments. When an endpoint sets `residency`, replicas with no `site` are excluded
- `InferenceProfile.gpu.count` is 1 to 8. The reconciler reserves that whole group on one node.
- Deployment revision 1 is written at create. Later revisions and replica sets are the rollout record described above
- HF tokens stay on the host (`HF_TOKEN`); never baked into guest images. Model jobs run outside the HTTP request: `Registered`, `Resolving`, `Downloading`, `Verifying`, `Scanning`, `Optimizing`, `Ready`, or `Failed` with a durable message and retry count. Scanning counts files (`FLUXVM_AI_MODEL_MAX_FILES`, default 10000) and checks the signature. Optimizing writes a derived artifact and does not replace the source digest. A second job for the same digest joins the first. A partial file is resumed. The job is refused when free disk is below the declared size. `GET /api/ai/model-jobs` and `GET /api/ai/models/{name}/status` report that state. `POST /api/ai/models/{name}/materialize` only records a job. `POST /api/ai/models/{name}/replicate` marks a model ready when a node at the site already lists it, or when `AiSite.peer_url` accepts the digest and returns 201. A missing peer, file, or token stays pending.

## Multi-site (Phase 5)

Tag replicas with `FLUXVM_AI_SITE` (or deployment `preferred_site`). Endpoints
may set `preferred_site`, `allowed_sites`, and `residency`. Routing strategy
`site_local` biases Maglev weights toward the preferred site and never
includes backends outside `residency` / `allowed_sites`. A replica with no
`site` is excluded when `residency` or `allowed_sites` is set.

## Agent Runtime convergence (Phase 6)

Agents can call Fabric-managed inference without an external provider key:

```json
"fabric-qwen3-8b": {
  "kind": "fabric",
  "host": "10.96.0.50",
  "header": "authorization",
  "prefix": "Bearer ",
  "allowed_ports": [8000],
  "path_prefixes": ["/v1/"]
}
```

Set `ZYVOR_FABRIC_INFERENCE_BASE=http://10.96.0.50:8000` in the guest harness
so OpenAI-compatible CLIs hit the Maglev VIP through the egress broker.
Optional `FABRIC_AI_API_KEY` injects an endpoint key created via
`fabricctl ai key create`.

## Golden image runbook

CUDA and vLLM are **not** installed at boot via `apt`. Bake them into a qcow2.

Helper (needs `virt-customize` / libguestfs-tools on a GPU host):

```bash
./scripts/bake-ai-vllm-image.sh \
  /var/lib/libvirt/images/ubuntu-24.04-cloud.qcow2 \
  /var/lib/zyvor-fabricd/images/vllm-cuda.qcow2
# then: FLUXVM_AI_IMAGE=/var/lib/zyvor-fabricd/images/vllm-cuda.qcow2
```

Manual path:

1. Start from an Ubuntu cloud image with NVIDIA driver + CUDA matching the host.
2. Install vLLM into a venv (or system path) and verify `vllm serve --help`.
3. Install the FluxVM guest agent.
4. Snapshot as `/var/lib/zyvor-fabricd/images/vllm-cuda.qcow2`.
5. Set `FLUXVM_AI_IMAGE` to that path on the fabricd host.

Cloud-init only writes `/etc/systemd/system/vllm.service` pointing at the
mounted model directory (`/models`) and starts it.

### Model materialization

- Public Hugging Face: Fabric downloads on the host, verifies checksum, bind-mounts into the guest.
- Private: read `HF_TOKEN` from the **host** environment (same boundary as the Agent Runtime credential broker). Never bake the token into the image.
- Offline / lab: set `FLUXVM_AI_MODEL_DIR=/path/to/weights` so create skips the download. That stub directory is not part of the managed cache.
- `POST /api/ai/models/cache/evict` with `budget_bytes` (or `FLUXVM_AI_MODEL_CACHE_BYTES`) deletes the oldest unreferenced directories under `{state}/ai-models`. A model named by any deployment is kept. The artifact record remains so it can be materialized again.

### GPU-less smoke

Set `FLUXVM_AI_DRY_RUN=1` on fabricd to exercise REST/CLI without bind/create.
Deployments report `DryRun` with synthetic replicas. CI and lab smoke use this
mode when Janus is unset. For a lab GPU stand-in, deploy Janus
(`./scripts/deploy-remote.sh HOST USER` from the Janus repo), set
`FLUXVM_AI_JANUS_URL=http://127.0.0.1:30818` and
`FLUXVM_AI_JANUS_API_KEY` on fabricd, and leave dry-run on. Fabric places on
`janus:node-0:gpu-0` and proxies chat to that NodePort. Maturity stays **Preview**.

## FluxVM prerequisites

```bash
# On the GPU host
curl -sS http://127.0.0.1:7788/v1/host/gpus/preflight | jq
# Bind only when placing (Fabric reconciler does this):
# POST /v1/host/gpus/bind  {"bdf":"0000:01:00.0","vram_gib":24}
```

Hardware gate: `fluxvm/scripts/test-gpu-passthrough-lifecycle.sh`.

## Console

`/app/ai` — Models, Deployments, Endpoints, API keys, Nodes (Janus or registered
hosts), and Host GPUs (FluxVM PCI inventory). Capacity strip shows
free/total GPUs; deployments show autoscale bounds and Maglev weights; endpoints
show the `/api/ai/openai/{name}` gateway path.

## CI

GitHub Actions workflow `.github/workflows/ai-workloads.yml` runs on every PR
and push that touches AI paths, the unit/smoke scripts, AI docs, or the operator
admission webhook chart:

1. **Unit tests** — `scripts/test-ai-workloads-unit.sh` runs
   `cargo test -p zyvor-fabricd --lib api::ai::` (Janus inventory, MIG catalog,
   inference audience shape, admit policy, Raft lease counters, routing,
   rollouts, keys, limits).
2. **Janus filters** — the same job also runs the focused filters
   `api::ai::janus::`, `api::ai::gpu_orch::`, and
   `api::ai::gateway::tests::gateway_serves_openai_paths`.
3. **Dry-run smoke** — builds `zyvor-fabricd`, starts it with
   `FLUXVM_AI_DRY_RUN=1` and no Janus URL (synthetic bodies stay on), and runs
   `scripts/smoke-ai-workloads-phases.sh`. On failure it uploads `fabricd.log`.
4. **Operator chart** — `helm template` with the webhook disabled (default) and
   with `admissionWebhook.enabled=true` plus a token, so the ValidatingWebhook
   URL includes `/api/ai/admit/{token}`.
5. **Agent credentials** — `cargo test` for Agent Runtime fabric credential wiring.

Lab smoke is the same phase script. Janus health and a proxied chat stay on the
lab host; CI does not start a Janus NodePort. On a lab with
`FLUXVM_AI_JANUS_URL` set, run
[`scripts/smoke-ai-janus-lab.sh`](../scripts/smoke-ai-janus-lab.sh) for nodes,
gateway chat, MIG catalog, and admit. Workflow file:
[`.github/workflows/ai-workloads.yml`](../.github/workflows/ai-workloads.yml).
