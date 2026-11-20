# Tutorial 15: AI Workloads (Beta)

Deploy an OpenAI-compatible inference endpoint on Fabric — from model
registration through chat completions — using either a real NVIDIA GPU VM
or the Janus lab stand-in when the host has no GPU.

**Level:** Intermediate  
**Time:** 35–50 minutes  
**Maturity:** Single-cluster **Beta** (not GA). Multi-site HA store stays Preview.

**Prerequisites:**

- `zyvor-fabricd` reachable at `https://127.0.0.1:9095` (or your host URL)
- Admin credentials (`/var/lib/zyvor-fabricd/.admin_password`)
- `fabricctl`, `curl`, `jq`
- One of:
  - **Path A — Janus lab:** `FLUXVM_AI_JANUS_URL` set (for example
    `http://127.0.0.1:30818`) and FluxVM reporting no NVIDIA GPU
  - **Path B — Real GPU:** FluxVM `/v1/host/gpus` shows an NVIDIA device,
    a baked vLLM (or other runtime) qcow2 at `FLUXVM_AI_IMAGE`, and IOMMU/VFIO ready

---

## What you will learn

1. Authenticate and inspect GPU / inference-node inventory
2. Register a model, profile, deployment, and OpenAI endpoint
3. Create an API key and call `/v1/chat/completions`
4. (Optional) Create and delete a MIG slice; enable PCI MIG; admit webhook
5. Use the console Nodes tab at `/app/ai`

---

## Step 0: Shell setup

```bash
export FABRIC_URL="${FABRIC_URL:-https://127.0.0.1:9095}"
export ZYVOR_FABRIC_URL="$FABRIC_URL"

PASS="$(sudo cat /var/lib/zyvor-fabricd/.admin_password)"
export ZYVOR_FABRIC_TOKEN="$(
  curl -sk -X POST "$FABRIC_URL/api/auth/login" \
    -H 'content-type: application/json' \
    -d "{\"username\":\"admin\",\"password\":\"$PASS\"}" \
  | jq -r .token
)"
test -n "$ZYVOR_FABRIC_TOKEN" && test "$ZYVOR_FABRIC_TOKEN" != null

curl -sk -H "authorization: Bearer $ZYVOR_FABRIC_TOKEN" \
  "$FABRIC_URL/readyz" | jq '{ok, store}'
```

---

## Step 1: See what Fabric thinks the GPU plane is

```bash
fabricctl ai gpus -o json | jq .
fabricctl ai node list -o json | jq .
fabricctl ai capacity -o json | jq .
```

**Janus path.** When `FLUXVM_AI_JANUS_URL` is set and FluxVM has no NVIDIA
GPU, you should see a node such as `node-0` with BDF `janus:node-0:gpu-0`
and site `janus`. Leave deployment `preferred_site` unset (or set it to
`janus`). A preferred site of `lab` alone rejects the only free Janus device.

**Real GPU path.** You should see PCI BDFs (`0000:01:00.0`) from FluxVM.
PCI MIG create needs `FLUXVM_AI_PCI_MIG=1` (see Step 6).

Lab smoke that covers nodes → chat → MIG → admit:

```bash
FABRIC_URL="$FABRIC_URL" ./scripts/smoke-ai-janus-lab.sh
```

---

## Step 2: Register a model and a profile

```bash
# Model artifact (Hugging Face id or a pre-staged host path)
fabricctl ai model add demo-qwen \
  --source hf://Qwen/Qwen3-8B

fabricctl ai model list -o json | jq '.[].name'

# Profile: runtime + shape. Known runtimes launch by default:
# vllm, tensorrt-llm, triton, llama.cpp, tei
fabricctl ai profile add demo-24g \
  --runtime vllm \
  --gpu 1 --vram 24 --cpu 8 --memory 32

fabricctl ai profile list -o json | jq .
```

On Janus, the profile still describes the *logical* shape; Fabric does not
create a VFIO VM — the replica address becomes the Janus hostport.

To block a runtime: `FLUXVM_AI_DENY_RUNTIMES=triton`.  
For a strict allowlist: `FLUXVM_AI_ALLOW_RUNTIMES=vllm,tei`.

---

## Step 3: Deploy and expose an endpoint

```bash
# Do not set preferred_site=lab when only Janus GPUs exist
fabricctl ai deploy demo-qwen \
  --profile demo-24g \
  --replicas 1

fabricctl ai deployment list -o json | jq .

fabricctl ai endpoint expose demo-qwen \
  --openai-compatible \
  --routing least_queue

fabricctl ai endpoint list -o json | jq .
```

Wait until the deployment shows a ready replica. On Janus you should see a
ready address like `127.0.0.1:30818`. Maglev skips upsert when every ready
replica is Janus or `dry-run-*`; the OpenAI gateway still proxies.

Console: open `/app/ai` → **Deployments** / **Endpoints** / **Nodes**.

---

## Step 4: API key and first chat completion

```bash
# Capture the secret once — it is not shown again
fabricctl ai key create demo-key --endpoint demo-qwen-openai -o json \
  | tee /tmp/ai-key.json

KEY="$(jq -r '.secret // .key // .token // empty' /tmp/ai-key.json)"
# If the CLI prints the secret outside JSON, copy it from the create output.
ENDPOINT="$(fabricctl ai endpoint list -o json | jq -r '.[0].name // "demo-qwen-openai"')"

export OPENAI_BASE_URL="$FABRIC_URL/api/ai/openai/$ENDPOINT"
curl -sk "$OPENAI_BASE_URL/v1/chat/completions" \
  -H "authorization: Bearer $KEY" \
  -H 'content-type: application/json' \
  -d '{
    "model": "demo-qwen",
    "messages": [{"role": "user", "content": "Say hello in one short sentence."}]
  }' | jq .
```

**Janus** returns a virtual completion (for example
`Zyvor Janus virtual completion.`).  
**Real GPU** returns model output from the guest runtime on `:8000`.

OIDC alternative: a JWT whose issuer matches an enabled provider and whose
audience is exactly `fabric-inference`. Control-plane JWTs with any other
audience are rejected unless `FLUXVM_AI_GATEWAY_ACCEPT_JWT=1`.

---

## Step 5: Scale, drain, and capacity

```bash
fabricctl ai deployment scale demo-qwen --replicas 2
fabricctl ai capacity -o json | jq .
fabricctl ai deployment drain demo-qwen
# scale back when done draining
fabricctl ai deployment scale demo-qwen --replicas 1
```

Revisioned rolling / canary / blue-green:

```bash
fabricctl ai deployment rollout demo-qwen --strategy canary --canary-percent 10
```

---

## Step 6: MIG slices (optional)

**Janus (record-only — no nvidia-smi):**

```bash
# Free the parent if a deployment still holds it
fabricctl ai deployment delete demo-qwen || true

fabricctl ai node mig-create node-0 \
  --parent-bdf janus:node-0:gpu-0 \
  --profile 1g.10gb -o json | jq '.gpus[].bdf'

fabricctl ai node mig-delete node-0 'janus:node-0:gpu-0--1g.10gb--0'
```

**PCI (driver):**

```bash
# On fabricd host
# Environment=FLUXVM_AI_PCI_MIG=1
# Optional lab without nvidia-smi: FLUXVM_AI_PCI_MIG_RECORD_ONLY=1

fabricctl ai node mig-create <node-id> \
  --parent-bdf 0000:01:00.0 \
  --profile 1g.10gb
```

Without `FLUXVM_AI_PCI_MIG=1`, a PCI parent still returns **400**.

Catalog profiles: `1g.10gb`, `1g`, `2g.20gb`, `2g`, `3g.40gb`, `3g`,
`7g.80gb`, `7g`.

---

## Step 7: Admission webhook (optional, Kubernetes)

Fabric exposes `POST /api/ai/admit`. Lab k3s can enable the operator chart
value `admissionWebhook.enabled=true` so `InferenceDeployment` CRs are
validated against tenant policy (model allow-list, deploy window, sites).

```bash
# Direct admit check
curl -sk -X POST "$FABRIC_URL/api/ai/admit" \
  -H "authorization: Bearer $ZYVOR_FABRIC_TOKEN" \
  -H 'content-type: application/json' \
  -d '{"tenant":"lab","model":"demo-qwen"}' | jq .
```

Unreachable fabricd fails closed when the webhook is enabled.

---

## Step 8: Clean up

```bash
fabricctl ai key delete "$(jq -r .id /tmp/ai-key.json)" 2>/dev/null || true
fabricctl ai endpoint delete demo-qwen-openai 2>/dev/null || true
fabricctl ai deployment delete demo-qwen 2>/dev/null || true
fabricctl ai profile delete demo-24g 2>/dev/null || true
fabricctl ai model delete demo-qwen 2>/dev/null || true
```

---

## Environment cheat sheet

| Variable | Purpose |
|---|---|
| `FLUXVM_AI_JANUS_URL` | Janus OpenAI shim (lab GPU stand-in) |
| `FLUXVM_AI_JANUS_API_KEY` | Bearer forwarded to Janus |
| `FLUXVM_AI_DRY_RUN` | Synthetic gateway body; Janus still proxied |
| `FLUXVM_AI_IMAGE` | Guest qcow2 with the runtime binary |
| `FLUXVM_AI_PCI_MIG` | Allow PCI MIG via `nvidia-smi` |
| `FLUXVM_AI_PCI_MIG_RECORD_ONLY` | PCI MIG inventory without driver |
| `FLUXVM_AI_DENY_RUNTIMES` | Block known runtime names |
| `FLUXVM_AI_ALLOW_RUNTIMES` | Strict allowlist when set |
| `FLUXVM_AI_RAFT_ID` / `PEERS` / `TOKEN` | Three-process lease + rate/audit tip |

Full reference: [ai-workloads.md](../ai-workloads.md).

---

## Next steps

- Bake a CUDA + vLLM image: `scripts/bake-ai-vllm-image.sh`
- Terraform: `terraform-provider/examples/ai-workloads/`
- Operator CRs: `operator/examples/ai-inference-deployment.yaml`
- Agent Runtime with `kind: fabric` credentials for on-prem models
