# Zyvor Fabric — Product Positioning

**Zyvor Fabric** is the Linux control plane for private cloud infrastructure: VM lifecycle, networking, security, storage, HA, operators, Terraform, and monitoring — with VM execution handled by [FluxVM](https://github.com/zyvorai/fluxvm), a disposable-VM engine with no systemd dependency.

It is **not** positioned as a basic VM manager. It is a **VM operations fabric** and **private cloud control plane** — closer to Proxmox + KubeVirt UX, without the heavyweight multi-package stack.

Part of the [Zyvor](https://zyvor.dev) product family from ZyvorAI Labs.

---

## What Zyvor Fabric Is

| Dimension | Positioning |
|-----------|-------------|
| **Category** | Pluggable virtualization platform / private cloud control plane |
| **Analogy** | Proxmox-class UX + KubeVirt-style operations, single Rust daemon |
| **Runtime** | `zyvor-fabricd` — one binary, one config; runs under systemd (still fully supported) or standalone |
| **Scope** | VMs, network fabric, security policy, storage, HA, migration, observability |
| **Interfaces** | CLI (`fabricctl`), Web UI, K8s operator, Terraform provider |

### Elevator pitch

> Zyvor Fabric is a production-grade private cloud control plane for Linux. Deploy in minutes with a single daemon, manage everything through four interfaces, and get enterprise features — RBAC, HA, live migration, GPU passthrough, network policies — without VMware complexity or OpenStack overhead.

---

## Who this is for

| Persona | Role | What they care about | Where Fabric fits |
|---------|------|----------------------|--------------------|
| **Economic buyer** — VP Infrastructure / CTO at a mid-size org | Owns the build-vs-buy decision and the budget | Total cost of ownership vs. VMware/OpenStack licensing, operational headcount, vendor lock-in risk, time-to-value | One binary instead of a multi-server stack; Apache-2.0 core with paid enterprise support as an option, not a requirement — see [License & Support](#license--support) below |
| **Platform engineer** ("Morgan" in [USER_STORIES.md](USER_STORIES.md)) | Builds the infrastructure other teams consume | Kubernetes operator, Terraform provider, GitOps-friendly declarative config | `VirtualMachine` CRD + `fabricctl apply -f config.yaml`, both backed by the same REST API as the UI |
| **Private cloud admin** ("Alex" in [USER_STORIES.md](USER_STORIES.md)) | Day-2 VM lifecycle, backups, incident response | A UI and CLI that don't drift from each other, audit trail for every action | Web console + `fabricctl`, both hitting the identical 780+-endpoint API |
| **Developer** ("Jordan" in [USER_STORIES.md](USER_STORIES.md)) | Needs VMs for testing/dev without filing a ticket | Self-service, scriptable, fast | CLI + REST API, VM creation in one command |

The economic-buyer row is the one most positioning docs skip. The short version for that persona: Fabric is under the Zyvor Production License v1.0 (free for non-production use; production use needs a commercial license, sales@zyvor.dev); the cost you're actually evaluating against VMware/OpenStack is **operational** — one 15MB binary and one config file vs. hundreds to thousands of packages and multiple dependent services (see the [Comparison Matrix](guides/decision-support/comparison-matrix.md) for the itemized breakdown). Paid Enterprise support/SLAs are separate from the production license.

---

## Naming Model

| Layer | Name | Notes |
|-------|------|-------|
| **Product** | Zyvor Fabric | Marketing, UI, documentation, sales |
| **Daemon** | `zyvor-fabricd` | systemd unit, config paths, APIs — stable technical identifier |
| **CLI** | `fabricctl`, `zyvor-fabricd-ctl` | Operational tools |
| **Repo** | [zyvorai/fabric](https://github.com/zyvorai/fabric) | Canonical GitHub org repo |

Keeping `zyvor-fabricd` as the daemon name avoids breaking installs, Ansible roles, and automation. User-facing surfaces say **Zyvor Fabric**; ops runbooks reference `zyvor-fabricd` where commands and paths matter.

---

## Competitive Frame

### vs. Proxmox / VMware / OpenStack

- **Lighter** — single 15MB binary vs. hundreds (Proxmox) to thousands (OpenStack) of packages
- **No systemd dependency for VM lifecycle** — VMs run under FluxVM's own process supervision, not as systemd units
- **Rust control plane** — memory-safe by construction (zero `unsafe` Rust, zero shell pipelines); no garbage-collection pauses. We haven't published a benchmarked latency/memory figure with a documented method and environment, so we don't cite one here — see the [Comparison Matrix](guides/decision-support/comparison-matrix.md) for what *is* independently verifiable today (setup time, package count, config file count)
- **API-first** — 780+ REST endpoints, 3 WebSocket channels, full automation surface

### vs. "Another VM manager"

Zyvor Fabric includes:

- Network policies, VM firewalls, service mesh, QoS, DNS policy, VPN mesh, NAT, packet mirror
- HA clustering, DRS, fault tolerance, replication, site recovery
- Distributed storage backends (local, NFS, LVM, ZFS, Ceph)
- Kubernetes operator + Terraform provider
- 90+ web pages, security hardening (31-round audit, 194 issues found and fixed, 0 outstanding — [report](SECURITY_AUDIT_REPORT.md)), audit logging, compliance tooling

### When to look elsewhere

Fabric isn't the right fit for every case. See **[Is this for you?](why-fabric.md#is-this-for-you)** and the full **[Comparison Matrix](guides/decision-support/comparison-matrix.md)** for where Proxmox VE, oVirt, or a full libvirt stack currently outmatch it — chiefly: mature multi-host live migration at scale, existing deep libvirt/XML tooling investments, and first-class Windows guest support.

---

## Machina — macOS Companion (Roadmap)

**Machina** is a separate, **not-yet-shipped** product: an **AI-native Infrastructure Workbench for macOS**. Everything in this section is roadmap/vision, not a current capability of Zyvor Fabric.

> **Naming note:** the Zyvor ecosystem table (see the [Zyvor platform stack](platform-stack.md)) also lists a currently-shipping, lowercase **"machina"** described there as a physical bare-metal hypervisor OS (libvirt/KVM) — a different product from the macOS workbench described below. This naming collision predates this rewrite; flagging it here rather than guessing which name is authoritative, since that's a product-naming decision for whoever owns the Zyvor catalog, not something to resolve unilaterally in a docs pass.

| | Zyvor Fabric | Machina (roadmap) |
|---|-------------|---------|
| **Platform** | Linux hypervisor hosts | macOS desktop |
| **Role** | Control plane / data plane | AI operator + explorer UI |
| **Status** | Shipping | Not yet built — vision/roadmap only |
| **Analogy** | Proxmox backend | Lens + K9s + Copilot for infrastructure |
| **AI** | API hooks for automation | Local LLM, RAG, tool-calling into Fabric APIs |

### Machina vision

```
┌───────────────────────────────┐
│           Machina             │
├───────────────────────────────┤
│ AI Operator                   │
│ Infrastructure Explorer       │
│ Kubernetes                    │
│ VMs                           │
│ Containers                    │
│ Networking                    │
│ Storage                       │
│ Observability                 │
└───────────────────────────────┘
```

Machina would consume Zyvor Fabric APIs (`zyvor-fabricd`) and eventually Kubernetes, Terraform, and metrics backends. It would not replace the Linux daemon — it's conceived as the intelligent desktop shell in front of it. None of this exists yet; don't represent it as available to a buyer.

### Roadmap (Machina, if/when built)

| Version | Focus |
|---------|-------|
| **v0.1** | VM dashboard, AI chat, metrics, logs |
| **v0.2** | Network topology, AI RCA, AI recommendations |
| **v0.3** | Kubernetes support, Terraform generation, incident analysis |
| **v0.4** | Local LLM, Infrastructure Time Machine, security advisor |
| **v1.0** | Full AI Infrastructure OS — multi-cluster, VM + K8s + bare metal |

Zyvor Fabric's own roadmap continues independently of Machina: HA, networking depth, operator maturity, and enterprise hardening on Linux.

---

## Messaging Guidelines

### Say

- "Zyvor Fabric — private cloud control plane for Linux, with a pluggable VM driver"
- "VM operations fabric for Linux"
- "Proxmox-class capabilities, single-daemon simplicity"
- "Part of the Zyvor product family"

### Avoid

- "VM spawn tool" / "VM manager" as primary positioning
- Renaming `zyvor-fabricd` in install docs, systemd units, or paths
- Presenting the macOS Machina workbench as shipping — it is roadmap only
- Conflating the roadmap Machina (macOS) with the shipping lowercase "machina" bare-metal-OS product, or with Fabric (Linux), in the same install flow
- Citing "sub-millisecond" or specific RSS/latency numbers without a documented benchmark method and environment to back them

### Technical docs pattern

First mention in a section:

> **Zyvor Fabric** (`zyvor-fabricd`) provides …

Then use the appropriate name for context (product in prose, `zyvor-fabricd` in commands).

---

## License & Support

Zyvor Production License v1.0 (SPDX: LicenseRef-Zyvor-Production-1.0), applied to the entire repository. Non-production use is free; production use requires a commercial license from Zyvor AI Labs Private Limited. Production support, SLAs, and Zyvor Enterprise products are licensed and sold separately — contact [sales@zyvor.dev](mailto:sales@zyvor.dev). Full text: [README.md — License](../README.md#license).

---

## Links

- Product: [zyvor.dev](https://zyvor.dev)
- Repository: [github.com/zyvorai/fabric](https://github.com/zyvorai/fabric)
- Naming: [NAMING.md](NAMING.md)
- Comparison matrix: [guides/decision-support/comparison-matrix.md](guides/decision-support/comparison-matrix.md)
- Documentation: [docs/index.md](index.md)
- Client decks: [client-presentations/](client-presentations/)
