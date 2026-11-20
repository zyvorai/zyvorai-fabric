# Zyvor Fabric System Architecture

This document describes the architecture of Zyvor Fabric, a comprehensive virtual machine
management platform built on the Linux KVM hypervisor, with VM lifecycle owned by
[FluxVM](https://github.com/zyvorai/fluxvm) -- a disposable-VM engine with no
systemd dependency of its own. Zyvor Fabric provides a production-grade REST API,
web UI, and CLI for managing the full lifecycle of virtual machines.

---

## Table of Contents

1. [High-Level Architecture](#high-level-architecture)
2. [Component Overview](#component-overview)
3. [Crate Dependency Graph](#crate-dependency-graph)
4. [Request Lifecycle](#request-lifecycle)
5. [VM Lifecycle](#vm-lifecycle)
6. [Storage Architecture](#storage-architecture)
7. [Networking Stack](#networking-stack)
8. [Security Architecture](#security-architecture)
9. [Background Task System](#background-task-system)
10. [Web UI Architecture](#web-ui-architecture)
11. [Enterprise Features](#enterprise-features)

---

## High-Level Architecture

```
                         +---------------------+
                         |     Web Browser      |
                         | (React + TypeScript) |
                         +----------+----------+
                                    |
                            HTTPS / WSS
                                    |
                         +----------v----------+
                         |   Reverse Proxy /    |
                         |   TLS Termination    |
                         |   (nginx / caddy)    |
                         +----------+----------+
                                    |
                         +----------v----------+
                         |      Zyvor Fabric        |
                         |   (Axum + Tokio)     |
                         |                      |
                         |  +----------------+  |
                         |  | REST API       |  |
                         |  | 780+ endpoints |  |
                         |  +----------------+  |
                         |  | WebSocket      |  |
                         |  | Console / VNC  |  |
                         |  +----------------+  |
                         |  | SSE Events     |  |
                         |  +----------------+  |
                         |  | Auth Middleware |  |
                         |  | (JWT + PAM)    |  |
                         |  +----------------+  |
                         |  | Background     |  |
                         |  | Tasks (20+)    |  |
                         |  +----------------+  |
                         +----+-----+-----+----+
                              |     |     |
              +---------------+     |     +---------------+
              |                     |                     |
   +----------v----------+  +------v------+  +-----------v-----------+
   |  VM Driver (FluxVM) |  | State Store |  | networking (netlink)  |
   +----------+-----------+  +------+------+  +-----------+-----------+
              |                     |                     |
   +----------v----------+  +------v------+  +-----------v-----------+
   |  FluxVM (registry) |  | SQLite      |  | Linux Kernel          |
   +----------+-----------+  +------+------+  +-----------+-----------+
              |                     |                     |
   +----------v----------+  +------v------+  +-----------v-----------+
   |  QEMU / KVM          |  | Filesystem  |  | nftables / tc         |
   |  (Hypervisor)        |  | /var/lib/   |  | (Firewall / QoS)      |
   +----------------------+  | Zyvor Fabric/   |  +-----------------------+
                             +-------------+
```

### Data Flow Summary

```
Client Request
    |
    v
+-- Axum Router (tower middleware stack) ---------------------+
|   CorsLayer -> TraceLayer -> TimeoutLayer -> AuthMiddleware |
+------------------------------------------------------------+
    |
    v
+-- Handler Function ---------+     +-- AppState (Arc) -------+
|  Extract Path/Query/Body    | --> |  store: StateStore       |
|  Validate input             |     |  driver: Arc<dyn VmDriver>|
|  Acquire per-VM lock        |     |  config: Config          |
|  Call driver / state store  |     |  storage_manager         |
|  Emit audit log             |     |  policy_engine           |
|  Emit SSE event             |     |  service_mesh            |
|  Return JSON response       |     |  traffic_shaper          |
+-----------------------------+     |  dns_manager             |
                                    |  vm_firewall             |
                                    |  vpn_mesh                |
                                    |  packet_mirror           |
                                    |  nat_gateway             |
                                    |  net_monitor             |
                                    |  lock_manager            |
                                    |  event_tx (broadcast)    |
                                    |  shutdown (CancellToken)  |
                                    +--------------------------+
```

---

## Component Overview

### Zyvor Fabric - The Core Daemon

The `Zyvor Fabric` binary is the central daemon that orchestrates all operations. It is
structured as an Axum web server running on Tokio with the following subsystems:

| Subsystem         | Description                                                |
|-------------------|------------------------------------------------------------|
| REST API          | 780+ endpoints across 53 API modules                       |
| WebSocket         | Real-time VM console access via FluxVM's vsock guest agent |
| SSE Events        | Server-Sent Events for real-time VM state change delivery  |
| Auth Middleware    | JWT token validation with RBAC (Admin/User/Viewer)         |
| Background Tasks  | 20+ spawned tasks for reconciliation, monitoring, healing  |
| Plugin System     | Extensible plugin registry for custom integrations         |
| Quota System      | Resource quota enforcement with cached usage tracking      |

### State Store

The `state-store` crate provides persistent storage using atomic JSON file writes:

- **VM state**: Each VM is serialized as a JSON file under `/var/lib/zyvor-fabricd/vms/`
- **Entity storage**: Generic `save_entity` / `get_entity` / `list_entities` functions
  for all domain objects (snapshots, backups, templates, policies, etc.)
- **Atomic writes**: Uses write-to-temp + rename pattern to prevent corruption
- **In-memory cache**: VM state is mirrored in an `Arc<RwLock<HashMap>>` for fast reads
- **Path traversal protection**: Entity IDs are validated to reject `..`, `/`, `\`, `\0`
- **Paginated listing**: `list_vms_paginated(offset, limit)` with capped max (1000)

### VM Model

The `vm-model` crate defines the core data structures:

```
VM
  name: String          -- Unique identifier (1-64 chars, alphanumeric + . - _)
  state: VMState        -- Running | Stopped | Paused | Starting | Stopping | Failed | Unknown
  cpus: u32             -- 1-256 virtual CPUs
  memory: u64           -- Memory in MB (64 MB - 1 TB)
  disk: u64             -- Disk size in GB (default 20)
  image: String         -- OS image name or path
  ip: Option<String>    -- Assigned IP address
  pid: Option<u32>      -- QEMU process ID when running
  mac_address: Option   -- Generated MAC address
  hostname: Option      -- Guest hostname
  tags: Option<Vec>     -- Arbitrary string tags
  labels: Option<Map>   -- Key-value labels
  vnc_port: Option<u16> -- VNC display port
  created: DateTime     -- Creation timestamp (UTC)
  updated: Option       -- Last modification timestamp
  last_error: Option    -- Error from last async operation
```

### VMStartOptions

The `VMStartOptions` struct's field names still echo its origin as a mirror of
`systemd-vmspawn(1)`'s option set, but launch options are now translated into an
FluxVM `CreateVmRequest` rather than a `systemd-vmspawn` CLI invocation:

- Image source (directory, image file, raw disk)
- Hardware: CPUs, RAM, KVM toggle, vSock CID
- Firmware: UEFI, Secure Boot, SMBIOS, TPM
- Storage: bind mounts (translated into virtiofs shares, auto-mounted in the guest via a
  generated cloud-init entry -- see `fluxvm-driver::lifecycle`), extra drives
- Networking: user/TAP mode, MAC address
- Credentials and SSH key injection
- Pass-through environment variables

### Driver Architecture

The driver layer is a trait boundary (`VmDriver`, in `driver-core`) between the API
handlers and FluxVM, the sole VM backend -- `AppState.driver` is one
`Arc<dyn VmDriver>`, backed by `FluxVmDriver` and configured via `driver.fluxvm_url`/
`driver.fluxvm_token` in `zyvor-fabricd.toml`. `VmDriver` is a blanket implementation
over several component traits:

```
+-- VmDriver (driver-core) — blanket-impl'd over: --------------+
|  VMDriver           start/poweroff/terminate/reboot/get_state/|
|                      list_machines/get_properties/enable/     |
|                      disable/start_with_options/               |
|                      get_control_socket/get_mac_address        |
|  ResourceControlDriver  set_cpu_quota/set_memory_max/           |
|                      set_io_weight/freeze/thaw/is_frozen/       |
|                      set_pids_max/set_cpuset/get_cpuset         |
|  ResourceStatsDriver    get_metrics/get_pressure                |
|  LogDriver           stream_logs                                |
|  ImageDriver         list/clone/rename/remove/pull/import/      |
|                      export/clean images                        |
|  ShellDriver         shell/copy_to/copy_from                    |
|  ConsoleDriver       open_console (interactive shell)           |
|  CapabilityProvider  backend_name/has_resource_control           |
+------------------------------------------------------------------+
           |
           v
+-- FluxVmDriver ------------------------------------------------+
|  REST client to FluxVM (github.com/zyvorai/fluxvm), plus a  |
|  WebSocket dial for ConsoleDriver and a vsock-backed guest agent |
|  (via FluxVM) for shell/copy_to/copy_from. No systemd           |
|  dependency. A few ImageDriver operations (tar-format images)    |
|  intentionally error clearly -- a tar rootfs isn't a bootable    |
|  disk image for a real hardware VM, so there's no equivalent to  |
|  fake.                                                            |
+--------------------------------------------------------------------+
```

There is no live bind-mount-into-a-running-VM operation and no `machinectl`-only
fallback path anymore -- everything above goes through `state.driver`.

### Security Crate

Authentication and authorization are handled by the `security` crate:

- **PAM integration**: Authenticates users against the system PAM stack
  (service: `Zyvor Fabric` or fallback to `login`)
- **JWT tokens**: Issues and validates JSON Web Tokens with configurable expiration
- **Token revocation**: In-memory revoked token set (JTI-based)
- **RBAC**: Three roles -- Admin, User, Viewer
- **Extractors**: Axum extractors `RequireRead`, `RequireWrite`, `RequireAdmin`
  for declarative endpoint protection
- **User database**: SQLite-backed user store with bcrypt password hashing
- **Audit logging**: Structured audit log entries (user, action, resource, status)
- **External auth**: LDAP and OIDC provider integration

---

## Crate Dependency Graph

The workspace contains 53 crates organized into the following domains.
See [crate-map.md](crate-map.md) for the complete listing.

```
                                  Zyvor Fabric
                                     |
                 +-------------------+-------------------+
                 |                   |                   |
            Core Crates        Driver Crates       Feature Crates
                 |                   |                   |
         +-------+-------+    +-----+------+    +-------+-------+
         |       |       |    |     |      |    |       |       |
      vm-model  state  security  driver  fluxvm  networking  storage
               store            core    driver              manager
```

### Domain Groups

**Core** (4 crates): `zyvor-fabricd`, `vm-model`, `state-store`, `security`

**Drivers** (4 crates): `zyvor-fabric-vm-driver` (mkosi image building only), `zyvor-fabric-driver-core`, `zyvor-fabric-fluxvm-client`, `zyvor-fabric-fluxvm-driver`

**Networking** (12 crates): `networking`, `network-policy`, `service-mesh`, `service-lb`, `traffic-shaping`, `dns-policy`, `vm-firewall`, `vpn-mesh`, `packet-mirror`, `nat-gateway`, `net-monitor`, `zyvor-fabric-dnsmasq-manager`

**Storage** (2 crates): `zyvor-fabric-storage`, `distributed-storage`

**System** (4 crates): `zyvor-fabric-system`, `zyvor-fabric-vm`, `zyvor-fabric-lock-manager`, `zyvor-fabric-cgroup`

**Management** (14 crates): `lifecycle-manager`, `certificate-manager`, `resource-pools`, `encryption`, `site-recovery`, `replication`, `migration`, `predictive-drs`, `secrets-manager`, `compliance`, `billing`, `enterprise-identity`, `openstack-compat`, `host-lifecycle`

**Infrastructure** (6 crates): `datacenter`, `host-agent`, `fault-tolerance`, `content-library`, `tpm-support`, `k8s-pod-client`

**Utilities** (5 crates): `cloud-init`, `prometheus-exporter`, `vnc-proxy`, `ova-tools`, `api-error`

**CLI and UI** (2 crates + web): `fabricctl`, `zyvor-fabric-sdk`, plus the React web console (`web/`, not a Rust crate)

---

## Request Lifecycle

A typical API request flows through the following stages:

```
1. TCP Connection
   |
   v
2. TLS Termination (reverse proxy or built-in)
   |
   v
3. Axum Router Matching
   - CorsLayer (origin validation)
   - TraceLayer (request/response logging)
   - TimeoutLayer (request timeout enforcement)
   |
   v
4. Authentication Middleware
   - Extract Bearer token from Authorization header
   - Validate JWT signature and expiration
   - Check if token JTI is revoked
   - Inject Claims into request extensions
   |
   v
5. Authorization Extractors
   - RequireRead:  all roles pass
   - RequireWrite: Admin and User pass
   - RequireAdmin: only Admin passes
   |
   v
6. Handler Function
   - Deserialize path params, query params, JSON body
   - Validate input (vm name, entity name, field ranges)
   - Acquire per-VM mutex lock (for state-changing operations)
   - Execute business logic via drivers / state store
   - Write audit log entry
   - Emit SSE event via broadcast channel
   - Return JSON response with appropriate status code
   |
   v
7. Response Serialization
   - JSON body with serde_json
   - Paginated responses: { items: [...], total, offset, limit }
   - Error responses: { error: "message" }
   - Path sanitization for non-admin error messages
```

### Error Handling

All handlers return structured JSON error responses:

```json
{
  "error": "VM 'test-vm' not found"
}
```

For non-admin users, file system paths and internal details are stripped from error
messages via `sanitize_error()` to prevent information leakage.

---

## VM Lifecycle

```
                    create_vm()
                        |
                        v
    +----------+   +---------+   +----------+
    |          |   |         |   |          |
    |  Failed  |<--| Stopped |-->| Starting |
    |          |   |         |   |          |
    +----+-----+   +----^----+   +-----+----+
         |              |              |
         |              |              v
         |         +----+----+   +---------+
         |         |         |   |         |
         +-------->| Stopping|<--| Running |
                   |         |   |         |
                   +---------+   +----+----+
                                      |
                                      v
                                 +---------+
                                 |         |
                                 | Paused  |
                                 |         |
                                 +---------+

State transitions:
  Stopped  -> Starting -> Running   (start_vm)
  Running  -> Stopping -> Stopped   (stop_vm)
  Running  -> Paused                (pause_vm)
  Paused   -> Running              (resume_vm)
  Running  -> Stopping -> Stopped -> Starting -> Running (restart_vm)
  *        -> Failed               (on error)
  *        -> [deleted]            (delete_vm, only when Stopped/Failed)
```

### VM Creation Flow

```
1. Client POSTs to /api/v1/vms with CreateVMRequest
2. Handler validates name (regex: ^[a-zA-Z0-9][a-zA-Z0-9._-]{0,63}$)
3. Handler validates resource limits (CPUs 1-256, Memory 64MB-1TB, Disk 1GB-64TB)
4. Handler checks quota availability (if quotas configured)
5. VM model is created with state=Stopped, timestamps set
6. State persisted to /var/lib/zyvor-fabricd/vms/{name}.json
7. Cloud-init ISO generated if hostname/user-data provided
8. Prometheus counter incremented (zyvor_fabricd_vm_creates_total)
9. SSE event emitted: { type: "vm_created", name: "..." }
10. JSON response returned with full VM object
```

### VM Start Flow

```
1. Client POSTs to /api/v1/vms/{name}/start
2. Handler acquires per-VM mutex lock
3. State set to Starting, persisted
4. state.driver.start(name) -- FluxVmClient issues a REST call to the
     configured `fluxvm serve` instance, which launches the VMM
     (QEMU / Cloud Hypervisor / Firecracker / FluxVM hypervisor) directly
5. PID captured
6. State set to Running, PID stored, persisted
7. Prometheus gauges updated
8. SSE event emitted: { type: "vm_started", name: "..." }
```

---

## Storage Architecture

### File System Layout

```
/var/lib/zyvor-fabricd/
  |
  +-- vms/                     # VM state JSON files
  |   +-- web-server.json
  |   +-- db-primary.json
  |
  +-- images/                  # VM disk images (.raw, .qcow2)
  |   +-- fedora-40.raw
  |   +-- ubuntu-24.04.qcow2
  |
  +-- storage/                 # Storage pool metadata
  |   +-- pools/
  |   +-- volumes/
  |
  +-- snapshots/               # Snapshot metadata
  +-- backups/                 # Backup metadata and data
  +-- templates/               # VM templates
  +-- schedules/               # Scheduled task definitions
  +-- cloud-init/              # Generated cloud-init ISOs
  +-- certificates/            # TLS certificate store
  +-- auth.db                  # SQLite user database
  +-- .jwt_secret              # Persisted JWT signing secret (mode 0600)
  +-- .admin_password          # Generated admin password (mode 0600)

/etc/zyvor-fabricd/
  +-- zyvor-fabricd.toml            # Primary configuration file
```

Host networking (bridges, VLANs, taps, bonds, VXLANs) is applied directly via netlink,
not written to config files -- see [Networking Stack](#networking-stack).

### Storage Pools

The `Zyvor Fabric-storage` crate supports multiple storage backends:

| Type      | Description                        | Use Case                    |
|-----------|------------------------------------|-----------------------------|
| Local     | Local directory on host filesystem | Development, single node    |
| NFS       | Network File System mount          | Shared storage, migration   |
| LVM       | Logical Volume Manager             | Performance, snapshots      |
| LVM-Thin  | Thin-provisioned LVM               | Overcommit, fast snapshots  |
| ZFS       | ZFS pool                           | Data integrity, compression |
| Ceph      | Distributed Ceph RBD               | Multi-node, HA              |

### Storage Operations

- Pool lifecycle: create, start, stop, delete, health check, stats refresh
- Volume lifecycle: create, resize, attach to VM, detach, delete
- Image management: import (OVA/VMDK/VDI), download cloud images, build custom
- Online disk resize for running VMs
- Distributed storage with datastore clusters and SDRS recommendations

---

## Networking Stack

Zyvor Fabric includes a comprehensive networking stack spread across 12 crates:

```
+------------------------------------------------------+
|                    Application Layer                  |
|                                                       |
|  +-- service-mesh ------+  +-- dns-policy ----------+|
|  | Service discovery    |  | DNS zone management    ||
|  | Load balancing       |  | DNS record resolution  ||
|  | Health checks        |  | Per-VM DNS policies    ||
|  | Backend management   |  +-----------------------+ ||
|  +---------------------+                             |
+------------------------------------------------------+
|                    Policy Layer                       |
|                                                       |
|  +-- network-policy ----+  +-- vm-firewall ---------+|
|  | L3/L4 access control |  | Per-VM firewall rules  ||
|  | Identity-based rules |  | Firewall profiles      ||
|  | Policy enforcement   |  | Firewall zones         ||
|  +---------------------+  +-----------------------+  |
+------------------------------------------------------+
|                    Transport Layer                    |
|                                                       |
|  +-- traffic-shaping ---+  +-- vpn-mesh ------------+|
|  | QoS policies (tc)    |  | WireGuard tunnels      ||
|  | Bandwidth limits     |  | Overlay networks       ||
|  | Priority queues      |  | Mesh topology          ||
|  +---------------------+  +-----------------------+  |
+------------------------------------------------------+
|                    Infrastructure Layer               |
|                                                       |
|  +-- nat-gateway -------+  +-- packet-mirror -------+|
|  | SNAT / DNAT rules    |  | Traffic mirroring      ||
|  | NAT pools            |  | Mirror sessions        ||
|  | Gateway management   |  | Capture targets        ||
|  +---------------------+  +-----------------------+  |
|                                                       |
|  +-- net-monitor -------+  +-- networking ----------+|
|  | Bandwidth monitoring |  | Base bridge/tap setup  ||
|  | Alerting policies    |  | Interface management   ||
|  | Per-VM metrics       |  +-----------------------+ |
|  +---------------------+                             |
|                                                       |
|  +-- service-lb --------+  +-- dnsmasq-manager -----+|
|  | Service Fabric v6+   |  | Per-bridge DHCP server ||
|  | node select/rollout  |  | (supervised dnsmasq)   ||
|  +---------------------+  +-----------------------+  |
+------------------------------------------------------+
|                    Kernel Layer                       |
|                                                       |
|  netlink (rtnetlink)  |  nftables  |  tc (qdisc)     |
|  bridge / tap / dnsmasq |  WireGuard                 |
+------------------------------------------------------+
```

### Network Configuration via netlink

The `networking` crate manages host networking with direct `rtnetlink` calls
(no systemd-networkd dependency -- configuration takes effect immediately, there's no
config-file-then-reload step):

- **Bridges**: created/deleted directly via netlink
- **VLANs**: 802.1Q VLAN devices
- **TAP devices**: For VM network interfaces
- **macvtap**: Direct hardware passthrough
- **Bonds**: Link aggregation (active-backup, LACP)
- **VXLANs**: Overlay tunnels for multi-host networking
- **SR-IOV**: Hardware-assisted virtual functions
- **Port forwards**: NAT-based port forwarding rules
- **DHCP**: a per-bridge `dnsmasq` process the daemon spawns and supervises directly
  (`zyvor-fabric-dnsmasq-manager`), replacing systemd-networkd's built-in DHCP server
- **WireGuard mesh** (`vpn-mesh`): same netlink-based approach, plus `wg` CLI for key/peer
  configuration

Desired topology is persisted in the state store and replayed idempotently on daemon
startup, which is what previously relying on `.netdev`/`.network` files surviving a
reboot provided.

### Background Reconciliation

Each networking subsystem has a background reconciler that runs periodically
to detect and correct configuration drift:

| Reconciler              | Interval | Purpose                                |
|-------------------------|----------|----------------------------------------|
| policy_reconciler       | 30s      | Enforce network access policies        |
| service_health_checker  | 10s      | Check service mesh backend health      |
| service_reconciler      | 30s      | Synchronize service mesh state         |
| qos_reconciler          | 30s      | Apply traffic shaping rules            |
| dns_reconciler          | 30s      | Synchronize DNS zone records           |
| firewall_reconciler     | 30s      | Enforce firewall rules via nftables    |
| vpn_reconciler          | 30s      | Maintain WireGuard tunnel state        |
| mirror_reconciler       | 30s      | Manage packet mirror sessions          |
| nat_reconciler          | 30s      | Apply NAT gateway rules                |
| net_monitor             | 10s      | Collect per-VM network metrics         |

---

## Security Architecture

### Authentication Flow

```
+-- Client ---------+      +-- Zyvor Fabric --------------------+
|                    |      |                                 |
| POST /auth/sign-in  | ---> |  1. Lookup user in SQLite DB    |
| { username, pass } |      |  2. Verify bcrypt password hash |
|                    |      |  3. OR: PAM authenticate        |
|                    | <--- |  4. Generate JWT (24h default)  |
| { token: "eyJ..." }|      |  5. Return token + user info    |
+--------------------+      +---------------------------------+

+-- Client ---------+      +-- Zyvor Fabric --------------------+
|                    |      |                                 |
| GET /api/v1/vms   | ---> |  1. Extract Bearer token        |
| Authorization:     |      |  2. Validate JWT signature      |
|  Bearer eyJ...     |      |  3. Check expiration            |
|                    |      |  4. Check revocation (JTI)      |
|                    |      |  5. Extract role from claims    |
|                    | <--- |  6. Enforce RBAC on endpoint    |
+--------------------+      +---------------------------------+
```

### RBAC Model

| Role    | Read | Write | Manage | Example Operations                       |
|---------|------|-------|--------|------------------------------------------|
| Viewer  | Yes  | No    | No     | List VMs, view metrics, read logs        |
| User    | Yes  | Yes   | No     | Create/start/stop VMs, manage snapshots  |
| Admin   | Yes  | Yes   | Yes    | User management, system config, quotas   |

### JWT Token Structure

```json
{
  "sub": "admin",
  "role": "admin",
  "exp": 1713024000,
  "jti": "550e8400-e29b-41d4-a716-446655440000"
}
```

- `sub`: Subject (username)
- `role`: One of `admin`, `user`, `viewer`
- `exp`: Expiration timestamp (configurable, default 24 hours)
- `jti`: Unique token ID for revocation tracking

### Security Hardening

- **JWT secret persistence**: Secret is auto-generated and stored at
  `/var/lib/zyvor-fabricd/.jwt_secret` (mode 0600) so tokens survive daemon restarts
- **Admin password**: Never hardcoded; auto-generated and written to
  `/var/lib/zyvor-fabricd/.admin_password` (mode 0600)
- **Input validation**: All VM names, entity names, and IDs are validated with
  strict regex patterns to prevent command injection and path traversal
- **Error sanitization**: File paths and internal details are stripped from
  error messages returned to non-admin users
- **Per-VM locking**: Mutex-based per-VM locks prevent race conditions in
  concurrent state-changing operations; lock map is pruned at 10,000 entries
- **Broadcast channel backpressure**: SSE event channel has 256-slot buffer;
  slow consumers are dropped rather than blocking the system
- **WebSocket limits**: Maximum 50 concurrent connections, 64KB message size,
  5-minute idle timeout
- **Quota enforcement**: Per-user resource quotas with cached usage tracking
- **Audit logging**: All state-changing operations are logged with user,
  action, resource, and status fields

### External Authentication

In addition to built-in PAM + JWT authentication, Zyvor Fabric supports:

- **LDAP**: Bind to an LDAP directory for user authentication
- **OIDC**: OpenID Connect with any compliant identity provider
- **Multi-tenancy**: Project-based isolation with per-project member roles

---

## Background Task System

Zyvor Fabric uses a macro-based system to spawn and manage background tasks:

### spawn_bg! Macro

```rust
macro_rules! spawn_bg {
    ($state:expr, $name:expr, $func:expr) => {{
        let s = $state.clone();
        let token = $state.shutdown.clone();
        bg_tasks.push(tokio::spawn(async move {
            tokio::select! {
                _ = token.cancelled() => {
                    tracing::debug!("Background task '{}' cancelled", $name);
                }
                _ = $func(s) => {
                    tracing::debug!("Background task '{}' exited", $name);
                }
            }
        }));
    }};
}
```

Key properties:

- Each task receives a cloned `Arc<AppState>` and `CancellationToken`
- Tasks run until either completion or cancellation via the shutdown token
- On SIGTERM/SIGINT, the cancellation token is triggered
- Each task is given 5 seconds to finish before the process exits
- Tasks are fully independent; one task crashing does not affect others

### Active Background Tasks

| Task                  | Purpose                                          |
|-----------------------|--------------------------------------------------|
| schedule_checker      | Execute scheduled VM operations (cron-like)      |
| metrics_collector     | Gather VM and host resource metrics              |
| stale_host_detector   | Detect and mark unresponsive cluster hosts       |
| drs_executor          | Execute Distributed Resource Scheduler decisions |
| lock_renewal          | Renew distributed locks before expiration        |
| replication_scheduler | Trigger periodic VM replication sync             |
| ha_monitor            | Monitor VMs for high-availability failover       |
| vm_autohealer         | Restart failed VMs automatically                 |
| autoscaler            | Scale VM groups based on load metrics            |
| policy_reconciler     | Enforce network access control policies          |
| service_health_checker| Check service mesh backend health                |
| service_reconciler    | Reconcile service mesh desired vs actual state   |
| qos_reconciler        | Apply QoS / traffic shaping rules                |
| dns_reconciler        | Synchronize DNS zones and records                |
| firewall_reconciler   | Enforce per-VM firewall rules                    |
| vpn_reconciler        | Maintain WireGuard VPN tunnel state              |
| mirror_reconciler     | Manage packet mirror sessions                    |
| nat_reconciler        | Apply NAT gateway rules                          |
| net_monitor           | Collect network bandwidth metrics and alerts     |

### Graceful Shutdown

```
SIGTERM / SIGINT received
    |
    v
shutdown_signal() future completes
    |
    v
axum::serve graceful shutdown (finish in-flight requests)
    |
    v
CancellationToken::cancel() -- signals all background tasks
    |
    v
Wait up to 5 seconds per task (tokio::time::timeout)
    |
    v
Process exits
```

---

## Web UI Architecture

The web UI is a React 19 + TypeScript application built with Vite:

```
web/
  src/
    components/     -- React components
    pages/          -- Route-level page components
    hooks/          -- Custom React hooks
    api/            -- API client functions
    types/          -- TypeScript type definitions
```

### Technology Stack

| Technology    | Version | Purpose                              |
|---------------|---------|--------------------------------------|
| React         | 19.1    | Component framework                  |
| TypeScript    | 5.8     | Type safety                          |
| Vite          | 6.3     | Build tool and dev server            |
| Tailwind CSS  | 4.1     | Utility-first styling                |
| React Router  | 7.5     | Client-side routing                  |
| Recharts      | 2.15    | Dashboard charts and graphs          |
| xterm.js      | 5.5     | Terminal emulator for VM console     |
| noVNC         | 1.6     | VNC client for graphical VM console  |
| Lucide React  | 0.475   | Icon library                         |

### API Integration

The web UI communicates with Zyvor Fabric via:

- **REST API**: Standard fetch calls with JWT Bearer token
- **WebSocket**: VM console access (text terminal via xterm.js)
- **WebSocket**: VNC proxy for graphical console (via noVNC)
- **SSE**: Real-time event stream for VM state change notifications

The UI is served as static files by Zyvor Fabric itself via `tower-http::ServeDir`,
so no separate web server is needed for development or production.

---

## Enterprise Features

Zyvor Fabric includes enterprise-grade features that provide parity with commercial
hypervisor management platforms:

### Datacenter Management
- Multi-datacenter, multi-cluster, multi-host hierarchy
- Host registration, heartbeat monitoring, maintenance mode
- Automated host discovery

### Distributed Resource Scheduler (DRS)
- Automatic VM placement based on resource availability
- Load balance analysis and migration recommendations
- Affinity and anti-affinity rules
- Predictive analytics via the `predictive-drs` crate

### High Availability
- Fault tolerance with automatic failover
- VM replication across sites
- Site recovery plans with planned migration and disaster recovery
- Test failover without production impact

### Content Library
- Centralized image and template repository
- Library synchronization across sites
- Customization specs and host profiles
- Compliance checking

### Lifecycle Management
- Baseline definitions for host software versions
- Compliance scanning and remediation
- Rolling updates with pause/advance controls

### Certificate Management
- Certificate Authority (CA) creation and management
- Certificate issuance, renewal, and revocation
- Automated rotation scheduling
- Security baseline compliance checking
- Hardware attestation verification
