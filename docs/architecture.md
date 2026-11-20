# Zyvor Fabric Architecture

## Overview

Zyvor Fabric is a virtual machine management platform built in Rust. It provides VM lifecycle management through a REST and WebSocket API, a React web UI, a CLI, a Kubernetes operator, and a Terraform provider, backed by [FluxVM](https://github.com/zyvorai/fluxvm), a disposable-VM engine with no systemd dependency (`driver.fluxvm_url` in `zyvor-fabricd.toml`). systemd itself is optional for the daemon's own packaging/init too -- it runs fine under systemd or any other supervisor.

## System Diagram

```
 +----------+   +----------+   +-----------+   +------------+
 | fabricctl |   |  Web UI  |   |    K8s    |   | Terraform  |
 |  (CLI)   |   | (React)  |   | Operator  |   | Provider   |
 +----+-----+   +----+-----+   +-----+-----+   +------+-----+
      |              |               |                 |
      +--------------+---------------+-----------------+
                                   |
                    +--------------v--------------+
                    |      zyvor-fabricd daemon         |
                    |  (Axum + Tokio async runtime)|
                    +-+--+--+--+--+--+--+--+--+---+
                      |  |  |  |  |  |  |  |  |
        +-------------+  |  |  |  |  |  |  |  +-------------+
        v                v  |  |  |  |  |  v                 v
   +---------+    +------+  |  |  |  |  |  +------+   +-----------+
   |cloud-init|   | VNC  |  |  |  |  |  |  | TPM  |   | WebSocket |
   |Generator |   |Proxy |  |  |  |  |  |  |Mgr   |   | Console   |
   +---------+    +------+  |  |  |  |  |  +------+   +-----------+
                             |  |  |  |  |
              +--------------+  |  |  |  +-----------+
              v                 v  |  v              v
        +---------+      +------+  |  +------+  +---------+
        |Prometheus|     |State |  |  |  HA  |  | Backup  |
        |Exporter  |     |Store |  |  |Cluster| | Restore |
        +---------+      +------+  |  +------+  +---------+
                                    v
                        +-----------------------+
                        |   VM Driver: FluxVM |
                        +-----------------------+
```

## Crate Structure

The backend is a Cargo workspace with 53 crates organized into functional areas.

### Core

| Crate | Purpose |
|-------|---------|
| `zyvor-fabricd` | Main daemon -- HTTP/WebSocket server, route registration, config loading |
| `zyvor-fabric-vm-driver` | Builds VM images via `mkosi` -- unrelated to VM lifecycle, which is entirely FluxVM's job |
| `vm-model` | Core data structures: VM definitions, state enums, request/response types |
| `state-store` | Persistent VM state with JSON storage, in-memory caching, file persistence |
| `fabricctl` | CLI -- scriptable command-line tool with JSON/YAML/table output |
| `encryption` | Encryption at rest |
| `certificate-manager` | TLS certificate management |

### Cloud and Console

| Crate | Purpose |
|-------|---------|
| `cloud-init` | NoCloud ISO generation for automated VM initialization |
| `vnc-proxy` | WebSocket-to-TCP VNC proxy for noVNC |
| `gpu-passthrough` | VFIO GPU passthrough (NVIDIA, AMD — generic PCI, no vGPU/Intel GVT-g) |

### High Availability

| Crate | Purpose |
|-------|---------|
| `ha` | etcd-based clustering and leader election |
| `migration` | Live and offline VM migration |
| `fault-tolerance` | Automatic failover and fencing |
| `replication` | Data replication across nodes |
| `site-recovery` | Disaster recovery plans and execution |
| `predictive-drs` | Distributed Resource Scheduling |

### Operations

| Crate | Purpose |
|-------|---------|
| `backup` | Backup/restore with retention policies |
| `scheduler` | VM scheduling (once, daily, weekly) |
| `lifecycle-manager` | VM lifecycle automation |
| `resource-pools` | Resource pool management |
| `datacenter` | Datacenter and cluster abstractions |
| `content-library` | Shared image and template repository |
| `prometheus-exporter` | Prometheus metrics endpoint |
| `host-agent` | Host-level agent for cluster management |

## Technology Stack

| Layer | Technology |
|-------|------------|
| Language | Rust (2021 edition) |
| Async runtime | Tokio 1.44 |
| Web framework | Axum 0.8 |
| Serialization | serde + serde_json |
| CLI | clap 4.5 |
| Web | React 19 + Vite |
| D-Bus | zbus 4 |
| Frontend | React 19 + TypeScript + Vite + TailwindCSS |
| Terminal emulator | xterm.js |
| VNC client | noVNC |
| Monitoring | Prometheus |

## API Surface

- **780+ REST endpoints** covering VM management, snapshots, storage, networking, auth, quotas, schedules, audit, analytics, backups, notifications, templates, tags, cloning, DRS, fault tolerance, replication, site recovery, content library, lifecycle, certificates, encryption, resource pools, distributed storage, datacenters, events, autoscaling, hotplug, and image building.
- **3 WebSocket endpoints** for console access, VNC proxying, and live event streaming.
- **OpenStack compatibility** prefixes on the same listen port: `/identity`, `/compute`, `/image`, `/network`, `/volume` (experimental façade — see [openstack-compat.md](openstack-compat.md)).
- **SCIM 2.0** at `/scim/v2` for enterprise provisioning (see [scim-identity.md](scim-identity.md)).

All native Fabric endpoints use JSON payloads and follow RESTful conventions.

## Data Flow

```
User --> CLI  /  Web UI / K8s Operator / Terraform / openstack CLI
                      |
                      v
     REST /api  ·  /scim/v2  ·  OpenStack /identity|/compute|…
                      |
                      v
               Core Daemon (zyvor-fabricd)
                 /          \
                v            v
          VM Driver      State Store (/var/lib/zyvor-fabricd/)
              |
              v
          FluxVM --> Virtual Machines
```

## Storage Layout

VM state and artifacts are stored under `/var/lib/zyvor-fabricd/`:

```
/var/lib/zyvor-fabricd/
  *.json              VM metadata and configuration
  images/             VM disk images
  tpm/                Per-VM vTPM state directories
  snapshots/          VM snapshot data
  backups/            Backup archives
  state/              Persistent daemon state
```

## systemd Integration (optional)

systemd is no longer required to install or run zyvor-fabricd -- packaging has no hard `Requires: systemd`, no sysusers.d/tmpfiles.d/preset (the daemon creates its own runtime directories at startup), and the backup/cleanup jobs that used to be systemd timers now run from an in-process tokio scheduler. The units below are still shipped for operators who choose to run under systemd anyway; nothing in packaging auto-enables or auto-starts them.

| Unit | Purpose |
|------|---------|
| `zyvor-fabricd.service` | Main daemon (`Type=simple`; systemd hardening via `ProtectSystem=strict`, capability bounding -- no socket activation, no watchdog) |

VMs themselves are never systemd units -- their lifecycle is owned by [FluxVM](https://github.com/zyvorai/fluxvm), which supervises each VM's QEMU / Cloud Hypervisor / Firecracker / FluxVM hypervisor process directly (see [the FluxVM driver guide](guides/vm-drivers/fluxvm.md)). There is no per-VM systemd unit template.

## Security Model

- Runs as root (required for VM management and networking)
- When run under systemd, its hardening directives further restrict filesystem and network access -- not required when run another way
- JWT-based API authentication with SQLite user store
- Auto-generated JWT secret persisted to `/var/lib/zyvor-fabricd/.jwt_secret` (mode `0600`)
- Auto-generated admin password written to `/var/lib/zyvor-fabricd/.admin_password` (mode `0600`) on first startup
- RBAC with three roles: Admin, User, Viewer
- TLS support for production deployments
- vTPM for guest attestation and secure boot
- Audit logging for all administrative actions
- Input validation on all user-facing parameters (VM names, IP addresses, paths, storage names)
- Parameterized SQL queries throughout (no injection risk)
- No shell pipelines — all subprocess calls use direct argument passing
