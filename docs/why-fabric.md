# Why Zyvor Fabric

What Fabric is, the problems it answers, and whether it fits you. Back to the [README](../README.md).

## What is Zyvor Fabric?

**Zyvor Fabric** is a production-grade private cloud control plane for Linux. One ~15MB Rust daemon (`zyvor-fabricd`) gives you VM lifecycle, software-defined networking, pluggable storage, security policy, and **OpenAI-compatible AI inference** — managed through four interfaces (**CLI, Web, Kubernetes operator, Terraform**) that all talk to the same API, so nothing drifts between them.

It targets the gap between two extremes: **manual QEMU/KVM + shell scripts** (no security, no multi-user access, doesn't scale) and **heavyweight multi-server stacks** (hundreds to thousands of packages, dedicated ops teams, days to stand up). Fabric deploys in about 5 minutes, runs on any Linux server with KVM — no vCenter, no systemd hard-requirement — and still ships the things enterprise buyers actually ask for: RBAC, audit logging, HA clustering, live migration, GPU passthrough, Maglev load balancing, and a 780+-endpoint REST API for automation.

Fabric doesn't implement VM execution itself — that's a deliberate design choice, not a gap. It's the orchestration, API, auth, and UX layer on top of two independent sibling projects: **[FluxVM](https://github.com/zyvorai/fluxvm)** (the VM engine) and **[GuestKit](https://github.com/zyvorai/guestkit)** (offline disk tooling). Each is independently useful and separately adoptable.

> **Naming:** the product is **Zyvor Fabric**; the daemon/unit/paths stay `zyvor-fabricd`. Canonical repo: [zyvorai/fabric](https://github.com/zyvorai/fabric). See [docs/NAMING.md](NAMING.md) and [docs/POSITIONING.md](POSITIONING.md).

**Feature guides:** **[User Feature Guide](zyvor-fabric-user-feature-guide.md)** — 55 features across 9 areas (also [PDF](zyvor-fabric-user-feature-guide.pdf)) · **[User manual](user/README.md)** — every console surface, page by page.

---

## Why Zyvor Fabric

| Problem | Zyvor Fabric answer |
|---------|---------------------|
| Private cloud usually means a heavy hypervisor stack | A lightweight, disposable VM engine underneath ([FluxVM](https://github.com/zyvorai/fluxvm)) — no systemd dependency, no vCenter |
| No unified API across interfaces | 780+ REST endpoints and 3 WebSocket channels, one daemon, four front doors |
| Scripting vs. GUI is usually either/or | CLI (`fabricctl`) + web console + Terraform + Kubernetes operator, all first-class |
| Enterprise needs RBAC, audit, and encryption | JWT auth, 3-tier RBAC, audit export, encryption at rest |
| GPU passthrough is bolted on elsewhere | Generic PCI/VFIO passthrough REST API on Linux KVM |
| Inference needs a second control plane | **AI Workloads (Beta)** — models, Maglev backends, OpenAI gateway, Janus lab GPU or real NVIDIA VMs |
| Guest images ship without your tooling | Offline image customization via [GuestKit](https://github.com/zyvorai/guestkit) |

Full capability tour and metrics: **[docs/PRODUCT_OVERVIEW.md](PRODUCT_OVERVIEW.md)**. Every feature, exhaustively: **[FEATURES.md](../FEATURES.md)**.

---

## Is this for you?

Zyvor Fabric is a strong fit when:

- **You don't want a systemd hard-requirement for VM lifecycle** — VMs run under [FluxVM](https://github.com/zyvorai/fluxvm)'s own process supervision, not as systemd units; host networking uses direct netlink calls. systemd stays fully supported as one option for supervising the `zyvor-fabricd` daemon process itself, for operators who want it.
- **You need API-first automation** — a 780+-endpoint REST API for infrastructure-as-code, CI/CD pipelines, or custom tooling, not a GUI-only or XML-RPC-only product.
- **You're running single-host or small-cluster deployments** — lightweight VM management without the operational overhead of full cluster orchestration platforms.
- **You're security-conscious** — PAM/LDAP/OIDC authentication, role-based access control, audit logging, and network policy enforcement are built in, not bolted on.
- **You want private inference next to the VMs** — register a model, deploy replicas, Maglev-weight them, and front them with API keys or `aud=fabric-inference` JWTs ([Tutorial 15](tutorials/15-ai-workloads.md)).

Look elsewhere when:

- **You need large multi-host clusters with mature live migration today** — Fabric's native live-migration transport is still preview; the disk-copy path is GA.
- **You're deep in an existing libvirt ecosystem** — Fabric talks to FluxVM's own REST API, not libvirt's XML domain definitions; migrating existing libvirt tooling means API adaptation, not a drop-in swap.
- **You need first-class Windows guest support** — Fabric focuses on Linux guests via QEMU/KVM; mixed Windows/Linux fleets are not yet a good fit.

Common questions: **[FAQ](quick-reference/faq.md)**.
