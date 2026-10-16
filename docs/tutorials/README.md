# Zyvor Fabric Tutorials

Step-by-step guides for learning the Zyvor Fabric VM management platform.
Each tutorial builds on concepts from the previous one, but they can also be
followed independently.

## Prerequisites

All tutorials assume:

- Zyvor Fabric is running on a Linux host with systemd v260+
- You have a valid JWT token (see Tutorial 06 for authentication details)
- `curl` and `jq` are installed
- The host has KVM support (`/dev/kvm` exists)

Set these shell variables before starting any tutorial:

```bash
export FABRIC_HOST="https://127.0.0.1:9095"   # or http://localhost:9095
export TOKEN="your-jwt-token-here"

# Optional smoke before any tutorial:
curl -sk "$FABRIC_HOST/readyz" | jq '{ok, store, fluxvm_ok: .fluxvm.ok}'
```

---

## Tutorial Index

| #  | Title                          | Level        | Time   | Description                                                        |
|----|--------------------------------|--------------|--------|--------------------------------------------------------------------|
| 01 | [Your First VM](01-first-vm.md)                  | Beginner     | 30 min | Create, start, connect to, and tear down a VM end-to-end (`tenant`, `/readyz`). |
| 02 | [VM Networking](02-networking.md)                 | Intermediate | 45 min | Bridges, VLANs, bonds, port forwarding, network policies, and DNS. |
| 03 | [Snapshots & Backups](03-snapshots-backups.md)    | Intermediate | 30 min | Point-in-time snapshots, backup policies, and disaster recovery.   |
| 04 | [Advanced VM Configuration](04-advanced-vm-options.md) | Intermediate | 40 min | VMStartOptions, hotplug, disk resize, cloud-init, and credentials. |
| 05 | [Multi-Node Clustering](05-clustering.md)         | Advanced     | 60 min | Datacenters, clusters, resource pools, migration, HA, and DRS.     |
| 06 | [Security Hardening](06-security-hardening.md)    | Advanced     | 45 min | PAM auth, RBAC, JWT, firewalls, encryption, certs, and auditing.   |
| 07 | [Logging & Compliance](07-logging-compliance.md)  | Intermediate | 35 min | Journals, compliance scans, and secrets management.                 |
| 08 | [OpenStack Clients](08-openstack-clients.md)      | Beginner     | 25 min | Point `openstack` CLI / curl at Fabric’s Keystone/Nova façade.     |
| 09 | [VM Edge Dataplane](09-edge-dataplane.md)         | Intermediate | 60 min | FluxVM Network Fabric schema v4 via Fabric API / CLI / console.    |
| 10 | [Service Fabric Maglev VIP](10-service-fabric.md) | Intermediate | 35 min | Maglev VIP + v6 identity/L7 policy via Edge Dataplane Services.   |
| 11 | [Agent Runtime Quickstart](11-agent-runtime-quickstart.md) | Intermediate | 40 min | Build a Node.js FluxVM template, deploy an agent, drive a session. |
| 12 | [Container Groups](../container-groups.md)       | Intermediate | 30 min | Kubernetes-style Pod groups on FluxVM Secure Containers, end to end. |
| 13 | [Deploy an Agent from the Console](13-deploy-agent-from-console.md) | Beginner | 20 min | Build a bundle with `fabric-agent build`, deploy and run it from the web console. |
| 14 | [How Users Interact with Agents](14-interacting-with-agents.md) | Beginner | 25 min | Use cases: ops, coding, research, cron, webhook, loop, approval, handoff, IDE chat, and fan-out. |
| 15 | [AI Workloads (Beta)](15-ai-workloads.md) | Intermediate | 45 min | Model → profile → deploy → OpenAI chat; Janus lab GPU or real NVIDIA; MIG and admit. |
| 16 | [Keep workstation](16-merion-workstation.md) | Intermediate | 45 min | Keep 0.1: Keep mode, signed policy, merionctl, cockpit, browser view, live lab gate; CI stub without KVM. |
| 17 | [Keep PDF brief](17-merion-pdf-brief.md) | Beginner | 20 min | One-click PDF → brief.md; prove 0 CONNECT via audit + FluxVM host eBPF (no PacketWolf). |

Deep-dive series (groups, CNP, observe, UX):
[edge-dataplane/](edge-dataplane/README.md).

**Keep** product docs: [docs/merion/MERION.md](../merion/MERION.md) · [confine](../merion/confine.md) · [PDF demo](../merion/demos/pdf-brief.md) · production gate: [docs/merion/PRODUCTION.md](../merion/PRODUCTION.md) · CI: [`.github/workflows/merion.yml`](../../.github/workflows/merion.yml) · lab: `./scripts/merion-live-lab.sh` · demo: `./scripts/merion-demo-pdf.sh`.

---

## Conventions Used

- **`$FABRIC_HOST`** -- Base URL of the Zyvor Fabric API (default `https://127.0.0.1:9095` in production-style deploys; older examples may use port 3000 for a local UI proxy)
- **`$TOKEN`** -- A valid Fabric JWT bearer token obtained via `/api/auth/login` (Tutorials 01–07)
- **`$OS_TOKEN` / `OS_*`** -- OpenStack Keystone token and env vars (Tutorial 08)
- All `curl` examples include `-s` (silent) and pipe through `jq` for readability; use `-sk` against lab self-signed TLS
- Response bodies show the **essential fields**; actual responses may include additional metadata
- UUIDs in examples are illustrative; your IDs will differ

## Quick Authentication Setup

```bash
# Log in and capture the token
TOKEN=$(curl -s "$FABRIC_HOST/api/auth/login" \
  -H "Content-Type: application/json" \
  -d '{"username": "admin", "password": "your-password"}' | jq -r '.token')

echo "$TOKEN"
```

---

## Feedback

Found an issue or have a suggestion? Open an issue on the project repository.
