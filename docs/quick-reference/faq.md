# Frequently Asked Questions

---

## General

### What is Zyvor Fabric?

Zyvor Fabric is a source-available virtual machine management platform built in Rust. It
provides a REST API, web UI, and CLI for managing the full lifecycle of virtual
machines on Linux hosts, using QEMU (and Cloud Hypervisor/Firecracker) via
[FluxVM](https://github.com/zyvorai/fluxvm), a disposable-VM engine with
no systemd dependency -- see "What is the role of FluxVM?" below.

### How is Zyvor Fabric different from libvirt/virt-manager?

Zyvor Fabric talks to FluxVM's own REST API instead of libvirt's abstraction
layer -- no libvirtd, no XML domain definitions. Host networking always uses
direct netlink calls rather than systemd-networkd, and DHCP is served by a
directly-managed `dnsmasq` process per bridge. Neither zyvor-fabricd nor
FluxVM has a systemd dependency; systemd is optional only as a way to
supervise the zyvor-fabricd *process* itself, for operators who choose that
supervisor.

### What VM driver is Zyvor Fabric built on?

VM lifecycle is handled entirely by [FluxVM](https://github.com/zyvorai/fluxvm),
reached over its REST API (`driver.fluxvm_url` in `zyvor-fabricd.toml`).
This covers VM lifecycle, cgroup resource control, log streaming, hotplug,
image management, shell exec, file copy, SSH info, and interactive console --
all through the same zyvor-fabricd API. Tar-format images are the one
exception: they're not supported and never will be, since a tar rootfs isn't
a bootable disk image for a real hardware VM (that model only worked for
`systemd-nspawn`'s shared-kernel containers, not for actual VMs).

### What hypervisor does Zyvor Fabric use?

Zyvor Fabric uses QEMU with KVM hardware acceleration by default (Cloud
Hypervisor and Firecracker are also available through FluxVM). FluxVM
launches and supervises each VM's hypervisor process directly -- there's no
systemd-vmspawn or any other intermediary managing VM lifecycle.

### What operating systems can Zyvor Fabric manage?

Zyvor Fabric can run any operating system that QEMU/KVM supports, including Linux,
Windows, FreeBSD, and others. The host must be Linux; there is no systemd
version requirement for VM lifecycle.

### How many VMs can Zyvor Fabric manage?

There is no hard-coded limit. The practical limit depends on host hardware
resources (CPU, memory, disk, network). The per-VM lock map is pruned at 10,000
entries, and the WebSocket connection limit is 50 concurrent sessions. The
pagination API supports listing up to 1,000 VMs per request.

### Is Zyvor Fabric production-ready?

Zyvor Fabric includes production-grade features: JWT authentication with RBAC, audit
logging, input validation and sanitization, per-VM locking, graceful shutdown,
Prometheus metrics, and comprehensive error handling. It has undergone multiple
rounds of security auditing. See the production deployment guide for recommended
configuration.

---

## Architecture

### Why is the backend written in Rust?

Rust provides memory safety without garbage collection, strong type system
guarantees, excellent async performance via Tokio, and low resource overhead.
These properties are well-suited for a systems management daemon that handles
concurrent VM operations, network configuration, and real-time event streaming.

### What is the role of FluxVM?

[FluxVM](https://github.com/zyvorai/fluxvm) is the disposable-VM control
plane zyvor-fabricd's `FluxVmDriver` speaks to over REST (`driver-core`'s
`VmDriver` trait). It launches and supervises each VM's QEMU/Cloud
Hypervisor/Firecracker process directly, tracks state in its own JSON-file
store, and exposes cgroup delegation, log capture, image catalog, and a
vsock-based in-guest agent (shell exec, file copy, interactive console) --
all with no systemd dependency of its own.

### Does Zyvor Fabric still use systemd-machined or systemd-vmspawn?

No. Both are fully removed. VM lifecycle is FluxVM only (`driver.fluxvm_url`).
The old `/api/machines` / machinectl UI is gone — use **Virtual Machines**
(`/app/vms`) and `/api/vms`. The `machinectl-driver` / `machined-dbus` crates
were deleted.

### Why are there 53 crates?

The workspace is organized into fine-grained crates to enforce clear module
boundaries, enable independent compilation and testing, and prevent circular
dependencies. Each crate has a focused responsibility: networking crates handle
their specific protocol, management crates handle their specific domain, and
driver crates abstract the hypervisor interface.

### How does the background task system work?

Zyvor Fabric uses the `spawn_bg!` macro to launch background tasks. Each task
receives a cloned `Arc<AppState>` and a `CancellationToken`. Tasks run as
independent Tokio tasks and are cancelled during graceful shutdown. There are
20+ background tasks handling reconciliation, monitoring, health checking,
auto-healing, and scheduled operations.

### How does the state store work?

The state store uses atomic JSON file writes (write to `.tmp` file, then
`rename()`) for crash safety. VM state is also cached in an
`Arc<RwLock<HashMap>>` for fast reads. Entity IDs are validated to prevent path
traversal. Each entity type is stored in its own subdirectory under
`/var/lib/zyvor-fabricd/`.

---

## Security

### How does authentication work?

Zyvor Fabric supports multiple authentication methods:
1. **Built-in**: Users stored in a SQLite database with bcrypt password hashing
2. **PAM**: Authentication delegated to the system PAM stack
3. **LDAP**: Bind authentication against an LDAP directory
4. **OIDC**: Token-based authentication with OpenID Connect providers

All authenticated sessions use JWT tokens with configurable expiration.

### What are the RBAC roles?

There are three roles:
- **Admin**: Full access to all operations including user management
- **User**: Can create, start, stop, and manage VMs and resources
- **Viewer**: Read-only access to VM listings, metrics, and logs

### How are JWT tokens secured?

JWT tokens are signed with a secret that is either set explicitly via the
`ZYVOR_FABRICD_JWT_SECRET` environment variable or auto-generated and persisted to
`/var/lib/zyvor-fabricd/.jwt_secret` (file permissions 0600). Tokens include a unique
JTI (JWT ID) that enables per-token revocation. The default expiration is 24 hours.

### How are passwords stored?

User passwords are hashed with bcrypt before storage in the SQLite database. The
admin password is either set via the `ZYVOR_FABRICD_ADMIN_PASSWORD` environment variable
or auto-generated and written to `/var/lib/zyvor-fabricd/.admin_password` (file
permissions 0600). Passwords are never logged.

### How is input validation handled?

All user-supplied input is validated before use:
- VM names: 1-64 characters, alphanumeric plus `.`, `-`, `_`, must start with
  alphanumeric
- Entity names: 1-128 characters, alphanumeric plus `.`, `-`, `_`, space
- Entity IDs: No `/`, `\`, `..`, or null bytes (prevents path traversal)
- Resource limits: CPUs 1-256, memory 64MB-1TB, disk 1GB-64TB
- Error messages are sanitized for non-admin users to prevent information leakage

---

## Networking

### What networking modes are supported?

Zyvor Fabric supports:
- **Bridge mode**: VMs connect to a host bridge (br0) for direct network access
- **TAP mode**: Individual TAP devices for per-VM network isolation
- **VLAN**: 802.1Q VLAN tagging for network segmentation
- **VXLAN**: Overlay networking for multi-host environments
- **WireGuard VPN mesh**: Encrypted overlay networks
- **macvtap**: Direct hardware passthrough for performance
- **SR-IOV**: Hardware-assisted virtual functions for near-native performance
- **Bond**: Link aggregation for redundancy

### How does the firewall work?

Zyvor Fabric provides per-VM firewall management:
1. Create firewall profiles with ingress/egress rules
2. Assign profiles to VMs
3. Rules are enforced via nftables
4. A background reconciler ensures rules stay in sync

### Can VMs communicate across hosts?

Yes, using VXLAN tunnels, WireGuard VPN mesh, or physical network bridging. The
service mesh provides service discovery and load balancing across hosts.

---

## Storage

### What storage backends are supported?

Zyvor Fabric supports six storage pool types:
- **Local**: Directory on the host filesystem
- **NFS**: Network File System mounts
- **LVM**: Logical Volume Manager
- **LVM-Thin**: Thin-provisioned LVM (supports overcommit)
- **ZFS**: ZFS pools with compression and data integrity
- **Ceph**: Distributed Ceph RBD for multi-node environments

### What image formats are supported?

- Raw disk images (`.raw`)
- QCOW2 (`.qcow2`) -- with snapshot and thin provisioning support
- OVA import (`.ova`)
- VMDK import (`.vmdk`)
- VDI import (`.vdi`)

### Can I resize a VM's disk while it is running?

Yes. The `/api/v1/vms/{name}/disk/resize` endpoint supports online disk resize
for running VMs. The guest OS must support online resize (most modern Linux
distributions do).

### How do snapshots work?

Zyvor Fabric snapshots capture the VM's disk state at a point in time. Snapshots are
stored as metadata in the state store and the actual disk delta is managed by the
underlying storage backend (e.g., QCOW2 overlay or LVM snapshot). You can revert
to any snapshot, and snapshot trees are supported.

---

## Operations

### How do I access a VM's console?

Two methods:
1. **Text console**: WebSocket connection at `/api/v1/ws/{vm_name}/console` using
   xterm.js in the web UI
2. **Graphical console**: VNC proxy via noVNC in the web UI, connecting through
   the VNC port assigned to the VM

### How do I monitor VM performance?

Multiple monitoring options:
- **Per-VM metrics**: `GET /api/v1/vms/{name}/metrics`
- **System analytics**: `GET /api/v1/analytics/system`
- **Prometheus endpoint**: `GET /metrics` for integration with Prometheus/Grafana
- **Network metrics**: `GET /api/v1/network-metrics/{name}`
- **Web UI dashboards**: Real-time charts via Recharts

### How do backups work?

Zyvor Fabric provides an API-driven backup system:
- Create on-demand backups via `POST /api/v1/backups`
- Schedule automated backups with backup policies (cron syntax)
- Restore from any backup via `POST /api/v1/backups/restore`
- Track backup jobs and view statistics

### Can I automate VM operations?

Yes, several automation features:
- **Schedules**: Cron-like scheduled operations (start, stop, backup)
- **Autoscale**: Automatic VM scaling based on resource utilization
- **DRS**: Automated VM placement and migration for load balancing
- **Auto-healing**: Automatic restart of failed VMs
- **Declarative specs**: Apply YAML/JSON VM specifications via
  `POST /api/v1/vms/apply`

### How does live migration work?

Two paths:

1. **Disk-copy “live”** (`POST /api/migrations`) — iterative **rsync** of disk/config
   while the guest runs, then a short pause for final sync and cutover. This is
   *not* QEMU memory live migration. Track with `GET /api/migrations/{id}`;
   cancel with `POST /api/migrations/{id}/cancel`. See [migration.md](../migration.md).

2. **Native FluxVM** (`/api/vms/{name}/migration/native/*` + receivers) — VMM
   transport with target receivers. **Preview** until shared-disk KVM e2e is
   green; ownership and APIs: [FLUXVM-FABRIC-BOUNDARY.md](../FLUXVM-FABRIC-BOUNDARY.md).

### How do I enable 2FA?

Enable TOTP-based two-factor authentication in the configuration file:

```toml
[auth.totp]
enabled = true
```

Then each user sets up 2FA by calling `POST /api/v1/auth/2fa/setup`, which
returns a TOTP secret and provisioning URI for an authenticator app. After
scanning the QR code, the user confirms with `POST /api/v1/auth/2fa/verify`.
Subsequent logins require a `totp_code` field in addition to the username
and password. Backup codes are provided during setup for account recovery.

### How do I export a VM to OVA format?

Export a VM to an OVA archive for portability:

```bash
# Start the export
curl -s -X POST http://127.0.0.1:9095/api/v1/vms/my-vm/export/ova \
  -H "Authorization: Bearer $TOKEN" | jq .

# Download the OVA file
curl -s http://127.0.0.1:9095/api/v1/vms/my-vm/export/ova/download \
  -H "Authorization: Bearer $TOKEN" -o my-vm.ova
```

The OVA archive includes the VM disk image, an OVF descriptor with hardware
configuration, and a manifest file. The exported OVA can be imported into
Zyvor Fabric, VMware, or VirtualBox.

### How do I manage secrets?

Zyvor Fabric includes a built-in secrets manager for storing credentials, API keys,
and certificates. Secrets are encrypted at rest and accessible only to
authorized users.

```bash
# Create a secret
curl -s -X POST http://127.0.0.1:9095/api/v1/secrets \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name": "db-pass", "value": "s3cret", "description": "DB password"}' | jq .

# List secrets (values are never exposed)
curl -s http://127.0.0.1:9095/api/v1/secrets \
  -H "Authorization: Bearer $TOKEN" | jq .
```

Secrets can be injected into VMs via cloud-init using
`POST /api/v1/vms/{name}/secrets`.

### How do I scan for compliance?

Zyvor Fabric supports compliance scanning against security baselines such as CIS
Benchmarks, DISA STIG, and PCI-DSS:

```bash
# List available profiles
curl -s http://127.0.0.1:9095/api/v1/compliance/profiles \
  -H "Authorization: Bearer $TOKEN" | jq .

# Scan a VM
curl -s -X POST http://127.0.0.1:9095/api/v1/compliance/scan \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"vm_name": "my-vm", "profile_id": "cis-level1"}' | jq .
```

Scans produce findings with severity levels (critical, high, medium, low) and
remediation guidance. Enable automatic scanning with `compliance.auto_scan = true`
in the configuration file.

### How do I set up billing?

Enable billing and configure pricing in the configuration file:

```toml
[billing]
enabled = true
currency = "USD"
billing_cycle = "monthly"
cpu_rate = 0.01        # per vCPU per hour
memory_rate = 0.005    # per GB per hour
storage_rate = 0.0001  # per GB per hour
```

Once enabled, Zyvor Fabric meters resource usage per VM. View usage with
`GET /api/v1/billing/usage`, list invoices with `GET /api/v1/billing/invoices`,
and configure custom pricing tiers with `POST /api/v1/billing/pricing`.

### How do I view VM logs?

Zyvor Fabric provides centralized log aggregation for all VMs:

```bash
# Get logs for a specific VM
curl -s http://127.0.0.1:9095/api/v1/logs/my-vm \
  -H "Authorization: Bearer $TOKEN" | jq .

# Search across all VM logs
curl -s "http://127.0.0.1:9095/api/v1/logs?query=error&limit=50" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

Logs can also be streamed in real-time via SSE at
`GET /api/v1/logs/{vm_name}/stream`. Logs are sourced from FluxVM's captured
console output; raw serial console output has no journald-equivalent
per-line priority/unit metadata, so every entry is stamped uniformly.

### How do I connect iSCSI storage?

Enable and configure iSCSI in the configuration file:

```toml
[storage.iscsi]
enabled = true
initiator_name = "iqn.2026-01.com.example:Zyvor Fabric"
```

Then discover and connect to iSCSI targets:

```bash
# Discover targets on a portal
curl -s -X POST http://127.0.0.1:9095/api/v1/iscsi/discover \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"portal": "192.168.1.100:3260"}' | jq .

# Log in to a target
curl -s -X POST http://127.0.0.1:9095/api/v1/iscsi/sign-in \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"portal": "192.168.1.100:3260", "target": "iqn.2026-01.com.example:storage"}' | jq .
```

Connected iSCSI LUNs can be used as storage pool backends for VM disks.

### What is split-brain protection?

Split-brain occurs when cluster nodes lose communication and each partition
believes it is the sole authority, leading to conflicting state changes.
Zyvor Fabric prevents split-brain using quorum-based fencing:

- A cluster requires a majority of nodes (quorum) to accept write operations
- Nodes that lose quorum are automatically fenced and stop serving requests
- VMs on fenced nodes are restarted on healthy nodes after a configurable timeout
- The etcd-based leader election ensures only one controller is active at a time

Configure split-brain protection in the controller section:

```toml
[controller]
enabled = true
mode = "controller"
quorum_required = true
fencing_timeout_seconds = 30
```

---

## Configuration

### Where does Zyvor Fabric look for its config file?

Zyvor Fabric checks the following paths in order, using the first one found:
1. `/etc/zyvor-fabricd/zyvor-fabricd.toml`
2. `configs/zyvor-fabricd.toml` (relative to working directory)
3. `zyvor-fabricd.toml` (relative to working directory)

If no config file is found, default values are used.

### Can I run Zyvor Fabric without authentication?

Yes, set `auth.enabled = false` in the configuration file. This is acceptable for
local development but is strongly discouraged for any deployment accessible on a
network.

### How do I change the listen address?

Set `daemon.listen` in `zyvor-fabricd.toml`, or export `ZYVOR_FABRICD_LISTEN`:

```toml
[daemon]
listen = "0.0.0.0:9095"   # Listen on all interfaces
```

```bash
export ZYVOR_FABRICD_LISTEN=0.0.0.0:9095
```

For external access, always use TLS (or a reverse proxy with TLS termination).
When OpenStack clients or remote catalogs need a reachable URL, also set
`daemon.public_url` or `ZYVOR_FABRICD_PUBLIC_URL` (see
[openstack-compat.md](../openstack-compat.md) and
[Tutorial 08](../tutorials/08-openstack-clients.md)).

### How do I point OpenStack / Terraform at Fabric?

Follow [Tutorial 08](../tutorials/08-openstack-clients.md). Short version: set
`OS_AUTH_URL` to `https://HOST:9095/identity` (or your `public_url` +
`/identity`).

### How do I configure CORS for the web UI?

Set `daemon.cors_origins` in `zyvor-fabricd.toml`:
```toml
[daemon]
cors_origins = ["https://zyvor-fabric.example.com", "http://localhost:5173"]
```

---

## Troubleshooting

### Zyvor Fabric fails to start with "Failed to initialize FluxVM driver"

This means `driver.fluxvm_url` in `zyvor-fabricd.toml` doesn't point at a
reachable `fluxvm serve` instance (wrong URL, FluxVM not started yet, or
a firewall blocking the connection). Confirm FluxVM itself is up:
```bash
curl -sf http://127.0.0.1:7788/healthz
curl -sf http://127.0.0.1:7788/readyz | jq .
curl -sk https://127.0.0.1:9095/readyz | jq '{ok, store, fluxvm_ok: .fluxvm.ok}'
```
and start FluxVM if it isn't (see [FluxVM's own README](https://github.com/zyvorai/fluxvm#readme)
and [PRODUCTION.md](https://github.com/zyvorai/fluxvm/blob/main/docs/PRODUCTION.md)).

### VMs fail to start with permission errors

Ensure the Zyvor Fabric process has access to `/dev/kvm`:
```bash
sudo chmod 666 /dev/kvm
# Or add the Zyvor Fabric user to the kvm group:
sudo usermod -aG kvm zyvor-fabricd
```

### "Token expired" errors after daemon restart

If the JWT secret was not persisted (file write failed), tokens from the previous
session are invalid. Either:
- Set `ZYVOR_FABRICD_JWT_SECRET` explicitly in the environment
- Ensure `/var/lib/zyvor-fabricd/.jwt_secret` is writable

### Web UI shows "Network Error" or CORS errors

Check that `daemon.cors_origins` includes the URL where the web UI is served.
If using the Vite dev server, add `http://localhost:5173`.

### VM state shows "Unknown"

The VM may have been started outside of Zyvor Fabric, or FluxVM may have
lost track of it. Check `curl http://127.0.0.1:7788/v1/vms` and verify the VM
is registered.

### High memory usage from Zyvor Fabric process

The Zyvor Fabric process itself should use well under 1 GB. If memory is high:
- Check the number of cached entities in the state store
- Check WebSocket connection count (max 50)
- Check the broadcast channel for slow consumers
- Review background task logs for runaway loops

### Audit logs growing too large

Export and archive old audit logs:
```bash
curl -s http://127.0.0.1:9095/api/v1/audit/logs/export \
  -H "Authorization: Bearer $TOKEN" > audit-archive.json
```

Consider implementing log rotation or external log shipping.
