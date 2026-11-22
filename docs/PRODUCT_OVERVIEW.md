# Zyvor Fabric — Product Overview

This doc is the capability tour and the one authoritative metrics table for Zyvor Fabric. For the pitch and quick start, see the [README](../README.md); for who this is (and isn't) for, see [POSITIONING.md](POSITIONING.md); for the exhaustive feature checklist, see [FEATURES.md](../FEATURES.md).

## The Problem

Organizations running Linux infrastructure need a unified control plane for virtual machines, networking, storage, and security. Existing solutions are either:

- **Too heavy** — traditional virtualization stacks require complex multi-server deployments, dedicated storage infrastructure, and specialized operations teams
- **Too basic** — Manual QEMU/KVM management with shell scripts doesn't scale and lacks security, monitoring, or multi-user access
- **Too locked-in** — Cloud-only solutions (AWS, Azure, GCP) create vendor dependency with unpredictable costs

Zyvor Fabric fills the gap: a VM operations fabric that runs on any Linux server, with or without systemd, providing enterprise features without enterprise complexity.

---

## What Is Zyvor Fabric?

Zyvor Fabric is a production-grade private cloud control plane built in Rust. It provides a complete management layer over [FluxVM](https://github.com/zyvorai/fluxvm), a disposable-VM engine with no systemd dependency:

- **One binary, one config file** — deploys in under 5 minutes (`zyvor-fabricd`), with systemd support built in but not required
- **780+ REST API endpoints** with JWT authentication, RBAC, and audit logging
- **4 management interfaces** — CLI, web dashboard, Kubernetes operator, Terraform provider
- **Enterprise features** — HA clustering, live migration, GPU passthrough, backup/restore, network policies

---

## Key Differentiators

### 1. VM Driver (Not a Custom Hypervisor)

Zyvor Fabric runs entirely without systemd via [FluxVM](https://github.com/zyvorai/fluxvm), its own disposable-VM engine (see [the FluxVM driver guide](guides/vm-drivers/fluxvm.md) for the full capability matrix). This means:

- No custom kernel modules or hypervisor patches
- Works on Fedora, Ubuntu, Debian, RHEL, SUSE, and any other Linux distribution alike, with or without systemd
- Upstream-maintained VM lifecycle, not a fork

### 2. Single Binary, Zero Dependencies

One ~15MB binary, one config file, a SQLite user store, and about 5 minutes to set up. It runs on any Linux.

### 3. Security-First Architecture

The entire codebase has undergone a **31-round security audit**: 194 issues identified and fixed, 0 outstanding ([full report](SECURITY_AUDIT_REPORT.md)):

- **Zero unsafe Rust** — memory-safe by construction
- **Zero shell pipelines** — all subprocess calls use safe argument passing
- **JWT + RBAC** on every API endpoint (Admin/User/Viewer roles)
- **bcrypt password hashing** with auto-generated secrets (0600 file permissions)
- **Rate limiting** on authentication (5 attempts/5 min)
- **Input validation** on every user-facing parameter
- **Audit logging** on all VM lifecycle operations
- **Path traversal protection** with canonicalization
- **SQL injection prevention** — all queries parameterized

### 4. Rust's Structural Advantages

- **Memory safety by construction** — zero `unsafe` Rust in the codebase, so whole classes of memory-corruption bugs (use-after-free, buffer overflow) aren't possible the way they are in a C/C++ control plane
- **No garbage-collection pauses** — Rust has no GC, so there's no stop-the-world latency spike under load the way there can be in a JVM- or Python-based equivalent
- **Safe concurrency** — per-VM mutexes prevent race conditions, checked at compile time
- We have **not** published a benchmarked API-latency or memory-footprint figure with a documented method, load profile, and hardware environment — so unlike the items above (which are structurally true of the language and codebase), we don't cite a specific number here. If you need real numbers for a sizing decision, ask — we'd rather point you at a repeatable benchmark than a marketing figure.

---

## Capability Tour

The exhaustive, line-by-line checklist lives in **[FEATURES.md](../FEATURES.md)**. This section is the skim version — one paragraph per area.

**VM lifecycle** — create, start, stop, restart, pause, resume, delete, hibernate (suspend-to-disk), full/linked cloning with CoW, templates, declarative config (`fabricctl apply -f config.yaml`), VM import from VMDK/VDI/VHD, online disk resize.

**Storage** — 6 backends (Local, NFS, LVM, LVM-thin, ZFS, Ceph/RBD), volume CRUD, snapshots with retention, ZFS incremental replication, Ceph cluster health/RBD management, live storage migration between pools, built-in cloud image catalog (Ubuntu/Fedora/Debian/Alma).

**Networking** — Cilium-style label-based network policies; a separate VM-edge dataplane (FluxVM Network Fabric schema v4, TC/eBPF per-VM allowlists + Mbps/PPS + flow stats — see [network-fabric-architecture.md](network-fabric-architecture.md)); per-VM firewall profiles; virtual-IP service mesh; QoS/traffic shaping; DNS policy; WireGuard VPN mesh; packet mirroring; NAT gateway; per-VM bandwidth monitoring with alerts.

**Security & identity** — JWT auth, LDAP/OIDC/OAuth2 SSO, 3-tier RBAC on every endpoint, multi-tenancy with project isolation and quotas, API keys for service-to-service auth, TLS/HTTPS, audit logging with JSON/CSV export, encryption at rest, SCIM 2.0 provisioning.

**High availability** — etcd-based clustering with leader election, predictive DRS, affinity/anti-affinity placement rules, automatic failover and fencing, distributed storage replication, site recovery, resource overcommit policies.

**Monitoring & automation** — Prometheus metrics, analytics dashboard, multi-channel notifications (Email/Slack/Webhook with retry/Teams), VM scheduling, automated backups with retention, post-install smoke tests, deep health checks, database schema migrations.

**Console access** — browser-based terminal (xterm.js over the FluxVM vsock agent) and VNC via noVNC, both authenticated with the same JWT tokens as the API.

**Cloud & virtualization** — cloud-init (NoCloud), TPM/vTPM 1.2/2.0, GPU passthrough (NVIDIA/AMD PCI/VFIO), live migration (disk-copy path GA; native FluxVM transport preview), CPU pinning/NUMA optimization, dual-stack IPv6.

---

## Management Interfaces

### CLI (`fabricctl`)

Scriptable command-line tool with JSON/YAML/table output:

```bash
fabricctl list -o json
fabricctl create myvm --image=ubuntu.qcow2 --cpus=4 --memory=4G
fabricctl start myvm
fabricctl apply -f infrastructure.yaml
fabricctl policy list
fabricctl ceph health my-pool
```

### Web Dashboard

Hybrid UI: public marketing pages (`/`, `/product`, `/platform`, `/security`) and a light Apple-style console under `/app` (React 19, SF Pro / system UI fonts, command palette Ctrl+K, WebSocket updates, bulk operations). Sign in at `/sign-in`. The former `zyvorctl-tui` terminal dashboard has been removed — use the web console or `fabricctl`.

### Kubernetes Operator

Manage VMs as `VirtualMachine` CRDs with automatic reconciliation:

```yaml
apiVersion: zyvorfabric.io/v1
kind: VirtualMachine
metadata:
  name: web-server
spec:
  image: ubuntu-22.04.qcow2
  cpus: 4
  memory: 4096
```

### Terraform Provider

Declarative VM provisioning with full plan/apply workflow:

```hcl
resource "zyvor_fabric_vm" "web" {
  name   = "web-server"
  image  = "ubuntu-22.04.qcow2"
  cpus   = 4
  memory = 4096
}
```

---

## Architecture

```
                    +-----------+    +----------+    +-----------+    +------------+
                    |  fabricctl |    |  Web UI  |    |    K8s    |    | Terraform  |
                    |   (CLI)   |    | (React)  |    | Operator  |    | Provider   |
                    +-----+-----+    +----+-----+    +-----+-----+    +------+-----+
                          |               |                |                  |
                          +---------------+----------------+------------------+
                                            |
                              +-------------v-------------+
                              |    zyvor-fabricd daemon   |
                              |   REST API + WebSocket    |
                              +-------------+-------------+
                                            |
                                  VM Driver: FluxVM
```

---

## Deployment Models

### Single Server

One Linux server running Zyvor Fabric with local storage. Suitable for development, testing, small teams, and edge deployments.

**Requirements:** Linux, KVM, 4GB RAM minimum. No systemd requirement.

### Multi-Node Cluster

Multiple Zyvor Fabric nodes with etcd-based clustering, shared storage (NFS/Ceph), live migration, and HA failover.

**Requirements:** 3+ nodes, shared storage, etcd cluster

### Kubernetes-Managed

Zyvor Fabric nodes managed by the Kubernetes operator. VMs defined as CRDs alongside containerized workloads.

**Requirements:** Kubernetes cluster with zyvor-fabricd operator deployed

---


---

## Technology Stack

| Layer | Technology |
|-------|------------|
| Language | Rust (2021 edition) |
| Async Runtime | Tokio 1.44 |
| Web Framework | Axum 0.8 |
| Web UI | React 19 + TypeScript + Vite + TailwindCSS |
| VM Backend | FluxVM (no systemd dependency) |
| Monitoring | Prometheus |

---

## Project Statistics

Every figure below is counted directly from source, not estimated — see the method column.

| Metric | Value | Method |
|--------|-------|--------|
| Backend crates | 53 | `backend/Cargo.toml` workspace `members` count |
| Rust source files | 165 | file count |
| TypeScript source files | 130 | file count |
| Total lines of code | ~87,000 (60K Rust + 27K TypeScript) | line count |
| REST API endpoints (main API) | 780+ | deduped (path, HTTP method) pairs parsed from `backend/zyvor-fabricd/src/server.rs`'s route registrations (785 exact at time of writing) |
| REST API endpoints (incl. OpenStack-compat shim) | 824 | adds 39 endpoints from `backend/openstack-compat/` |
| WebSocket endpoints | 3 | console, VNC, events |
| Web pages | 92 (87 console + 5 marketing) | React Router route entries in `web/src/App.tsx`, excluding redirects and catch-alls |
| Security audit rounds | 31 | [SECURITY_AUDIT_REPORT.md](SECURITY_AUDIT_REPORT.md) |
| Security issues fixed | 194 (0 outstanding) | [SECURITY_AUDIT_REPORT.md](SECURITY_AUDIT_REPORT.md) — 19 critical + 42 high + 84 medium + 49 low |
| Test suite | Passing | CI |

Endpoint and page counts will drift a little release to release; treat the "+"-suffixed headline numbers as safe lower bounds and the exact figures in the Method column as the snapshot at time of writing.

---

## License

Zyvor Production Licence v2.0 for the entire repository: A 30-day trial is available on request from sales@zyvor.dev (a paid evaluation Order at no charge); production and any other use needs a paid Order under the Zyvor Production Licence v2.0. See [README.md — License](../README.md#license) for the full text and how Enterprise support fits alongside it.

---

## Getting Started

See the [README Quick Start](../README.md#quick-start) for the primary install path (`git clone` + `make build && sudo make install`, or `./scripts/ship`). The `zyvor-fabricd-ctl` wrapper below is an alternative entry point some environments use for one-command deploy plus day-2 operations:

```bash
./zyvor-fabricd-ctl deploy    # deploy everything, auto-sudo
./zyvor-fabricd-ctl verify    # post-install smoke test (API, auth, VM CRUD, backups)
./zyvor-fabricd-ctl health    # deep health check (disk, DB, timers, resources)
./zyvor-fabricd-ctl backup now
```

Full command reference: `./zyvor-fabricd-ctl --help`.

---

*For technical details, see the [Architecture Guide](architecture.md), [API Reference](api.md), and [Security Documentation](security.md).*
