# Development Guide

This section covers everything you need to know to contribute to the Zyvor Fabric
project, from setting up your development environment to submitting pull requests.

---

## Documents

| Document                                   | Description                                        |
|--------------------------------------------|----------------------------------------------------|
| [Building](building.md)                    | Build prerequisites, compilation instructions, running individual crate tests, and building the web UI. |
| [Contributing](contributing.md)            | Development workflow, code style conventions, PR process, and architecture guidelines. |

---

## Quick Start

```bash
# Clone the repository
git clone https://github.com/zyvorai/fabric.git
cd fabric

# Build all backend crates
cd backend && cargo build

# Run the test suite
cargo test

# Build the web UI
cd ../web && npm install && npm run build

# Run the daemon locally (needs root for KVM/network access)
sudo ./backend/target/debug/zyvor-fabricd
```

---

## Repository Structure

```
zyvor-fabric/
  |
  +-- backend/                 # Rust workspace
  |   +-- zyvor-fabricd/          # Main daemon binary
  |   +-- fabricctl/               # CLI client
  |   +-- vm-model/            # Core data structures
  |   +-- state-store/         # Persistent state storage
  |   +-- security/            # Auth, JWT, PAM, RBAC
  |   +-- crates/driver-core/  # VmDriver trait family
  |   +-- crates/fluxvm-driver/   # FluxVM backend (no systemd dependency) -- the only VmDriver implementation
  |   +-- zyvor-fabric-vm-driver/   # mkosi image building (unrelated to VM lifecycle)
  |   +-- cloud-init/          # cloud-init ISO generation
  |   +-- prometheus-exporter/ # Prometheus metrics
  |   +-- vnc-proxy/           # WebSocket-to-VNC proxy
  |   +-- networking/          # Base network utilities
  |   +-- network-policy/      # L3/L4 access control
  |   +-- service-mesh/        # Service discovery
  |   +-- traffic-shaping/     # QoS management
  |   +-- dns-policy/          # DNS management
  |   +-- vm-firewall/         # Per-VM firewall
  |   +-- vpn-mesh/            # WireGuard VPN mesh
  |   +-- packet-mirror/       # Traffic mirroring
  |   +-- nat-gateway/         # NAT gateway
  |   +-- net-monitor/         # Network monitoring
  |   +-- datacenter/          # Multi-datacenter management
  |   +-- host-agent/          # Cluster host agent
  |   +-- resource-pools/      # Resource pool management
  |   +-- encryption/          # VM disk encryption
  |   +-- predictive-drs/      # Predictive DRS
  |   +-- distributed-storage/ # Distributed storage
  |   +-- fault-tolerance/     # HA and fault tolerance
  |   +-- replication/         # VM replication
  |   +-- migration/           # VM live migration
  |   +-- site-recovery/       # Disaster recovery
  |   +-- content-library/     # Content management
  |   +-- lifecycle-manager/   # Host lifecycle
  |   +-- certificate-manager/ # PKI management
  |   +-- tpm-support/         # TPM 2.0 support
  |   +-- crates/
  |   |   +-- driver-core/     # Driver trait definitions
  |   |   +-- fluxvm-driver/ # FluxVM VmDriver implementation
  |   |   +-- fluxvm-client/ # REST client for FluxVM (`readyz`, `list_vms_by_tenant`, `CreateVmRequest.tenant`)
  |   |   +-- storage/         # Storage pool management
  |   |   +-- system/          # System resource management
  |   |   +-- vm/              # VM-level utilities
  |   |   +-- lock-manager/    # Distributed locks
  |   |   +-- cgroup/          # Cgroup v2 integration
  |   +-- Cargo.toml           # Workspace definition
  |
  +-- web/                    # React + TypeScript web UI
  |   +-- src/                 # Source code
  |   +-- dist/                # Built static files
  |   +-- package.json         # npm dependencies
  |
  +-- docs/                    # Documentation
  |   +-- architecture/        # Architecture docs
  |   +-- deployment/          # Deployment guides
  |   +-- development/         # Development guides
  |   +-- quick-reference/     # Quick reference material
  |
  +-- configs/                 # Example configuration files
```

---

## Key Concepts

### AppState

All shared state is held in `AppState` (defined in `Zyvor Fabric/src/server.rs`),
which is wrapped in `Arc` and injected into every Axum handler via the `State`
extractor. Key fields include the state store, config, driver, all networking
subsystems, the event broadcast channel, and the shutdown cancellation token.

### Per-VM Locking

State-changing operations (start, stop, delete, etc.) acquire a per-VM mutex
from `AppState::vm_lock(name)`. This prevents race conditions when multiple
API calls target the same VM concurrently.

### Background Reconciliation

Many subsystems use background reconciler tasks that run on a periodic interval.
These tasks compare desired state (stored in the state store) with actual state
(queried from the kernel or systemd) and make corrections. All reconcilers are
launched via the `spawn_bg!` macro and participate in graceful shutdown.

### Driver Abstraction

`VmDriver` (an umbrella over `VMDriver`, `ResourceControlDriver`,
`ResourceStatsDriver`, `LogDriver`, `ImageDriver`, `ShellDriver`,
`ConsoleDriver`, and `CapabilityProvider`) in `driver-core` defines the
interface for VM lifecycle, cgroup resource control, log streaming, image
management, shell exec, and the interactive console. `FluxVmDriver` is
the only implementation — it talks to a separate
[FluxVM](https://github.com/zyvorai/fluxvm) process over its REST
API (`driver.fluxvm_url` in `zyvor-fabricd.toml`); no systemd
dependency at all.

This trait boundary is what makes testing with mock drivers possible.
It's also the reason the systemd-machined/systemd-vmspawn backend that
used to sit here (`MachinectlDriver`) could be deleted cleanly once
FluxVM covered every capability it provided.
