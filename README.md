<div align="center">

<img src="docs/social/fabric-hero-dark.jpg" alt="Zyvor Fabric - One daemon. Four front doors." width="100%">

# Zyvor Fabric

### Private cloud control plane for Linux.

VMs, networking, storage, security and AI inference from **one daemon**.<br>
One ~15MB Rust daemon that deploys in about 5 minutes on any Linux server with KVM.

[![CI](https://img.shields.io/github/actions/workflow/status/zyvorai/zyvorai-fabric/ci.yml?branch=main&style=flat-square&labelColor=1d1d1f&label=CI)](https://github.com/zyvorai/zyvorai-fabric/actions/workflows/ci.yml)
[![Keep](https://img.shields.io/github/actions/workflow/status/zyvorai/zyvorai-fabric/keep.yml?branch=main&style=flat-square&labelColor=1d1d1f&label=Keep)](https://github.com/zyvorai/zyvorai-fabric/actions/workflows/keep.yml)
[![Agent Runtime](https://img.shields.io/github/actions/workflow/status/zyvorai/zyvorai-fabric/agent-runtime.yml?branch=main&style=flat-square&labelColor=1d1d1f&label=Agent%20Runtime)](https://github.com/zyvorai/zyvorai-fabric/actions/workflows/agent-runtime.yml)
[![AI Workloads](https://img.shields.io/github/actions/workflow/status/zyvorai/zyvorai-fabric/ai-workloads.yml?branch=main&style=flat-square&labelColor=1d1d1f&label=AI%20Workloads)](https://github.com/zyvorai/zyvorai-fabric/actions/workflows/ai-workloads.yml)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-0071e3?style=flat-square&labelColor=1d1d1f)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-backend-0071e3?style=flat-square&labelColor=1d1d1f&logo=rust&logoColor=white)](backend/)
[![React](https://img.shields.io/badge/react-19.2-0071e3?style=flat-square&labelColor=1d1d1f&logo=react&logoColor=white)](web/)
[![Kubernetes](https://img.shields.io/badge/kubernetes-ready-0071e3?style=flat-square&labelColor=1d1d1f&logo=kubernetes&logoColor=white)](docs/KUBERNETES.md)
[![Built on FluxVM](https://img.shields.io/badge/VM%20engine-FluxVM-0071e3?style=flat-square&labelColor=1d1d1f)](https://github.com/zyvorai/zyvor-fluxvm)
[![Built on GuestKit](https://img.shields.io/badge/guest%20tooling-GuestKit-0071e3?style=flat-square&labelColor=1d1d1f)](https://github.com/zyvorai/zyvor-guestkit)

[**Quick start**](#quick-start) · [**Keep**](docs/keep/README.md) · [**AI Workloads**](docs/ai-workloads.md) · [**Deploy**](#deploy) · [**Docs**](docs/index.md) · [**Talk to Zyvor**](https://zyvor.dev)

</div>

---

## One daemon. Four front doors.

`zyvor-fabricd` gives you VM lifecycle, software-defined networking, pluggable storage, security policy and **OpenAI-compatible AI inference**. The CLI, the web console, the Kubernetes operator and Terraform all talk to the same API, so nothing drifts between them.

Fabric does not implement VM execution itself. That is a deliberate design choice, not a gap: **Fabric decides what should exist; [FluxVM](https://github.com/zyvorai/zyvor-fluxvm) makes it exist; [GuestKit](https://github.com/zyvorai/zyvor-guestkit) prepares the disk.**

> **Naming:** the product is **Zyvor Fabric**; the daemon, unit and paths stay `zyvor-fabricd`. Canonical repo: [zyvorai/fabric](https://github.com/zyvorai/zyvorai-fabric). See [docs/NAMING.md](docs/NAMING.md).

<div align="center">

![Zyvor Fabric dashboard](docs/assets/dashboard.png)

*The console dashboard — fleet health, capability status and live VM metrics at a glance.*

</div>

<div align="center">

[![Keep: your agent, your hardware, your keys](docs/assets/keep-hero.jpg)](docs/keep/README.md)

**Keep it yours.** *[Keep](docs/keep/README.md) is the sealed agent runtime you run, read and take with you. Muse column from its public announcement; evidence class today is `software-test`.*

</div>

<a id="ai-workloads-beta"></a>

<table>
<tr>
<td valign="top" width="33%">
<b>VMs and networking</b><br>
VM lifecycle, software-defined networking and a TC/eBPF VM-edge dataplane with per-VM policy and rate limits, on top of FluxVM.<br>
<a href="docs/network-fabric-architecture.md">Network Fabric</a>
</td>
<td valign="top" width="33%">
<b>Storage and security</b><br>
Pluggable storage. PAM/LDAP/OIDC sign-in, JWT auth, 3-tier RBAC, audit export and encryption at rest, built in.<br>
<a href="SECURITY.md">Security</a>
</td>
<td valign="top" width="33%">
<b>AI inference <sub>Beta</sub></b><br>
OpenAI-compatible inference on the same daemon: models, Maglev-weighted backends and API keys. Single-cluster Beta, not GA.<br>
<a href="docs/ai-workloads-at-a-glance.md">AI Workloads</a>
</td>
</tr>
<tr>
<td valign="top" width="33%">
<b>Keep</b><br>
An open agent workstation: drop a file, get answers, in a sealed microVM whose network policy the host sets. The docs say exactly what its evidence class does not prove.<br>
<a href="docs/keep/README.md">Keep</a>
</td>
<td valign="top" width="33%">
<b>Kubernetes, Operator, Terraform</b><br>
Run fabricd and FluxVM in a cluster with Helm or manifests; the operator turns <code>VirtualMachine</code> CRs into API calls; a Terraform provider is included.<br>
<a href="docs/KUBERNETES.md">Kubernetes</a>
</td>
<td valign="top" width="33%">
<b>Console and CLI</b><br>
A 92-page web console and <code>fabricctl</code>, both first-class against the same 780+-endpoint REST API.<br>
<a href="docs/web-ui.md">Web console</a>
</td>
</tr>
</table>

---

## Quick start

```bash
git clone https://github.com/zyvorai/zyvorai-fabric.git && cd fabric
make build && sudo make install
sudo zyvor-fabricd                                   # or: sudo systemctl enable --now zyvor-fabricd
fabricctl create web-01 --image fedora-41 --cpus 2 --memory 4096 --tenant acme
# Web UI → https://localhost:9095   (console at /app)
```

Default ports: **9095** (API + UI) and **7788** (FluxVM on localhost). The full path table, verification commands and multi-tenant notes are in [Getting started](docs/getting-started.md); a laptop dev setup is in [QUICKSTART.md](QUICKSTART.md).

<a id="is-this-for-you"></a>

**Is this for you?** It fits API-first automation, single-host or small-cluster deployments, security-conscious teams, and private inference next to the VMs. Look elsewhere for large multi-host clusters with mature live migration today, a deep libvirt-XML ecosystem, or first-class Windows guests. [The honest version](docs/why-fabric.md#is-this-for-you), the [comparison matrix](docs/guides/decision-support/comparison-matrix.md) and the [FAQ](docs/quick-reference/faq.md).

---

## Platform at a glance

| Metric | Value |
|--------|-------|
| Rust crates | 53 |
| REST endpoints | 780+ (main API) — 824 combined with the OpenStack-compatibility layer |
| LOC | ~87K (60K Rust + 27K TS) |
| Web stack | React 19.2 · Vite · Tailwind |
| Interfaces | 4 (CLI, Web, Operator, Terraform) + Fabric Doctor |
| Web pages | 92 (87 console + 5 marketing) |
| Security | 31-round audit, 194 issues found and fixed, 0 outstanding ([report](docs/SECURITY_AUDIT_REPORT.md)) |
| Deploy modes | Bare metal · Docker · Kubernetes · Operator |

All figures above are counted directly from source (route definitions, router config, crate manifest, audit report) — see [docs/PRODUCT_OVERVIEW.md](docs/PRODUCT_OVERVIEW.md) for the methodology.

---

## Deploy

Four first-class ways to run Fabric. Pick one; the full guide is [docs/deploy.md](docs/deploy.md).

- <a id="bare-metal-systemd--easiest-path"></a>**Bare metal (systemd), easiest path:** `./scripts/ship USER@HOST` ships FluxVM and Fabric in one command. [Details](docs/deploy.md#bare-metal-systemd--easiest-path).
- **Docker / Podman:** `make docker-up`, then `http://localhost:9095`. [Details](docs/DOCKER.md).
- <a id="run-on-kubernetes"></a>**Kubernetes:** `./scripts/deploy k8s USER@HOST` for a k3s lab, or Helm from `charts/zyvor-fabric`. [Details](docs/deploy.md#run-on-kubernetes) and [docs/KUBERNETES.md](docs/KUBERNETES.md).
- **Operator only:** CRDs that drive an already-running fabricd, from [`operator/`](operator/).

## Documentation

| Topic | Doc |
|---|---|
| Every document, by area | [docs/index.md](docs/index.md) · [docs/documentation-map.md](docs/documentation-map.md) |
| Getting started and deploy | [docs/getting-started.md](docs/getting-started.md) · [docs/deploy.md](docs/deploy.md) · [QUICKSTART.md](QUICKSTART.md) |
| Positioning, overview and every feature | [docs/POSITIONING.md](docs/POSITIONING.md) · [docs/PRODUCT_OVERVIEW.md](docs/PRODUCT_OVERVIEW.md) · [FEATURES.md](FEATURES.md) |
| Architecture | [docs/architecture.md](docs/architecture.md) · [docs/architecture-fluxvm-guestkit.md](docs/architecture-fluxvm-guestkit.md) |
| REST API, OIDC/SSO and networking | [docs/api.md](docs/api.md) · [docs/oidc.md](docs/oidc.md) · [docs/networking.md](docs/networking.md) |
| AI Workloads (Beta) | [docs/ai-workloads.md](docs/ai-workloads.md) · [Tutorial 15](docs/tutorials/15-ai-workloads.md) |
| Keep (open agent workstation) | [docs/keep/README.md](docs/keep/README.md) · [docs/keep/at-a-glance.md](docs/keep/at-a-glance.md) |
| Tutorials and user manuals | [docs/tutorials/README.md](docs/tutorials/README.md) · [docs/user/README.md](docs/user/README.md) |

## Go deeper

- <a id="architecture-fluxvm--guestkit"></a>**Architecture:** how Fabric composes FluxVM and GuestKit — [docs/architecture-fluxvm-guestkit.md](docs/architecture-fluxvm-guestkit.md).
- <a id="zyvor-platform-stack"></a>**Zyvor platform stack:** where Fabric sits among the Zyvor products — [docs/platform-stack.md](docs/platform-stack.md).

## Contributing

Contributions are welcome. See **[CONTRIBUTING.md](CONTRIBUTING.md)** for the development setup, code style and PR process.

```bash
make build          # backend + web
make test           # Rust + web tests
make lint && make fmt
```

`docs/` and this README are authoritative; historical build summaries in the repo root are snapshots.

## License

Commercial subscriptions and support: see [docs/SUBSCRIPTION-MODEL.md](docs/SUBSCRIPTION-MODEL.md).

### Open source (Apache-2.0)

This repository is licensed under the [Apache License, Version 2.0](LICENSE), in full — there is no dual-licensing or separately-licensed core component.
You may use, modify, and run it for personal, lab, and commercial production
use at no charge, subject to Apache-2.0 (preserve notices / NOTICE where required).
See [NOTICE](NOTICE).

### Enterprise

Production support, SLAs, and Zyvor Enterprise products are licensed separately.
Contact [sales@zyvor.dev](mailto:sales@zyvor.dev) or see [zyvor.dev](https://zyvor.dev).

<div align="center">

Part of the Zyvor platform. More at **[zyvor.dev](https://zyvor.dev)**.

</div>
