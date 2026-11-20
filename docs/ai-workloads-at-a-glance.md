# AI Workloads at a glance

The short version; the full reference is [ai-workloads.md](ai-workloads.md) and the walkthrough is [Tutorial 15](tutorials/15-ai-workloads.md). Back to the [README](../README.md).

## AI Workloads (Beta)

OpenAI-compatible inference on the same daemon — no separate AI control plane.

| | |
|---|---|
| **Maturity** | Single-cluster **Beta** · multi-site HA store stays Preview · **not GA** |
| **Console** | `/app/ai` — Models, Deployments, Endpoints, API keys, Nodes |
| **CLI** | `fabricctl ai model \| profile \| deploy \| endpoint \| key \| gpus \| node \| capacity` |
| **Gateway** | `/api/ai/openai/{endpoint}/v1/chat/completions` |
| **Lab without NVIDIA** | Set `FLUXVM_AI_JANUS_URL` — [Zyvor Janus](https://github.com/zyvorai/janus) is the virtual upstream |
| **Real GPUs** | FluxVM inventory + VFIO VM + runtime image (`FLUXVM_AI_IMAGE`) |
| **Runtimes** | `vllm`, `tensorrt-llm`, `triton`, `llama.cpp`, `tei` by default · deny/allow via env |
| **MIG** | Janus records always · PCI via `FLUXVM_AI_PCI_MIG=1` |

```bash
fabricctl ai model add demo-qwen --source hf://Qwen/Qwen3-8B
fabricctl ai profile add demo-24g --runtime vllm --gpu 1 --vram 24 --cpu 8 --memory 32
fabricctl ai deploy demo-qwen --profile demo-24g --replicas 1
fabricctl ai endpoint expose demo-qwen --openai-compatible
fabricctl ai key create demo-key --endpoint demo-qwen-openai
# → POST $FABRIC_URL/api/ai/openai/demo-qwen-openai/v1/chat/completions
```

**Guides:** [Tutorial 15 — how to use](tutorials/15-ai-workloads.md) · [Full reference](ai-workloads.md) · [Website tutorial](https://zyvor.dev/docs/zyvor-fabric-manual/ai-workloads) · Lab smoke: `./scripts/smoke-ai-janus-lab.sh`
