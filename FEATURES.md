# Zyvor Fabric Feature Reference

## Project Statistics

- 53 backend crates
- 165 Rust source files, 130 TypeScript files
- ~87,000 lines of code (60K Rust + 27K TypeScript)
- 780+ REST API endpoints + 3 WebSocket endpoints
- 92 web pages (87 console under `/app` + 5 marketing routes)
- 4 interfaces (CLI, Web, Operator, Terraform) — terminal UI removed
- 3 RBAC roles (Admin, User, Viewer)
- 4 disk formats (qcow2, raw, vmdk, vdi)
- 6 storage backends (Local, NFS, LVM, LVM-thin, ZFS, Ceph/RBD)

---

## VM Management

- Create, start, stop, restart, delete, clone VMs
- VM templates
- VM state persistence
- VM driver: [FluxVM](https://github.com/zyvorai/fluxvm), a disposable-VM engine with no systemd dependency (QEMU / Cloud Hypervisor / Firecracker / FluxVM hypervisor)
- **VM edge dataplane (FluxVM Network Fabric schema v4)** — proxied at `/api/vms/{name}/dataplane/*` and `/api/dataplane/{groups,cnp,health,ipcache,…}`; VM **Dataplane** tab (Status / Policy / Effective / Stats / Flows); console **Edge Dataplane**; Dashboard **VM dataplane** capability; `fabricctl dataplane` with groups/CNP/health. Orthogonal to Fabric SDN `/network-policies`. See [fluxvm-dataplane.md](docs/guides/vm-drivers/fluxvm-dataplane.md) and [user dataplane](docs/user/pages/infrastructure/dataplane.md).
- CPU and memory configuration (`--cpus`, `--ram`)
- Direct kernel boot (`--linux`, `--initrd`)
- TAP and user mode networking (`--network-tap`, `--network-user-mode`)
- Firmware selection (`--firmware`)
- Bind mounts as launch-time virtiofs shares (`--bind`, `--bind-ro`) — declared at VM create time, auto-mounted in the guest via cloud-init
- Core VM lifecycle, cgroup resource control (CPU/memory/IO/pids/cpuset, freeze/thaw), log streaming, CPU/memory/disk/NIC hotplug, image management, and interactive shell/console (real PTY, over the guest vsock agent)
- Multiple disk formats (qcow2, raw)
- VM tagging and grouping
- VM scheduling (once, daily, weekly)
- Lifecycle management
- Hotplug (CPU, memory, disk, NIC)

## User Interfaces

### CLI (fabricctl)

- Cilium-style grouped help with emoji section markers (`fabricctl --help`) plus 15+ subcommand groups (policy, firewall, service, qos, dns, vpn, mirror, nat, monitor, ceph, net, dataplane, …)
- `fabricctl dataplane status|policy|stats|flows|effective|health|group|cnp|observe|hubble|…` — FluxVM Network Fabric edge (not SDN network-policies)
- Meta: `status` (API/dataplane checklist), `config`, `completion` (bash/zsh/fish)
- Output formats: table, JSON, YAML (`-o json|yaml|table`); `--color auto|always|never` (TTY-aware, honors `NO_COLOR`)
- Declarative config import: `fabricctl apply -f config.yaml`
- Config export: `fabricctl export <resource> -o yaml`
- Ceph management: pool create, health, stats, RBD image CRUD
- Colorized tables/status and Hubble flow output

### Web UI (React)

- Hybrid: public marketing (`/`, `/product`, `/platform`, `/security`) + light Apple-style console under `/app`
- Sign-in at `/sign-in` (`/login` redirects)
- 87 console pages + network/security sub-tabs
- Command palette (Ctrl/Cmd+K), sequence shortcuts, left nav
- Dashboard with real-time statistics and capabilities health
- Cilium-style network security page with 9 tabs
- VM list, detail tabs, creation wizard, console/VNC
- First-run Getting Started panel when no VMs exist
- Undo window on VM deletion
- SF Pro / system UI font; Apple contrast tokens (`#1d1d1f`, `#333336`, `#6e6e73` on `#f5f5f7`)
- Tailwind CSS styling

The former terminal UI (`zyvorctl-tui`) has been removed.
## Console Access

### WebSocket Console

- Real-time browser-based terminal via xterm.js
- PTY streaming over FluxVM's vsock guest agent
- Full terminal emulation

### VNC

- VNC WebSocket proxy with noVNC web client
- Per-VM VNC configuration
- Dynamic port assignment

## Cloud-Init

- NoCloud seed generation via FluxVM's `CloudInitSpec`, attached on the VM's next (re)start
- User-data and meta-data support
- Network configuration
- SSH key injection
- Package installation (`packages`)
- Custom script execution (`runcmd`)
- File writes into the guest before first boot (`write_files`, with optional permissions)

## TPM / vTPM

- TPM 1.2 and 2.0 support
- swtpm integration
- Per-VM TPM instances
- TPM state persistence
- EK and platform certificates

## GPU Passthrough

- NVIDIA and AMD support
- VFIO integration

## Live Migration

- Cross-host orchestration belongs to Fabric; FluxVM owns VMM transport.
- **Runtime contract v1** adds typed FluxVM source-side QEMU pre-copy/post-copy/multifd primitives and capability discovery.
- Target-receiver preparation and full HA cutover remain pre-GA; the obsolete rsync + `machinectl` live path is disabled. <!-- ZYVOR_RUNTIME_BOUNDARY_V1 -->

## Security

### Authentication

- JWT-based authentication with auto-generated signing secret (persisted to `/var/lib/zyvor-fabricd/.jwt_secret`, mode `0600`)
- Auto-generated admin password on first startup (written to `/var/lib/zyvor-fabricd/.admin_password`, mode `0600`)
- Configurable via `ZYVOR_FABRICD_JWT_SECRET` and `ZYVOR_FABRICD_ADMIN_PASSWORD` environment variables
- User management with SQLite backend
- Password hashing (bcrypt with DEFAULT_COST)
- Token generation, validation, and configurable expiration
- API key support for service-to-service auth
- SCIM 2.0 provisioning for Entra ID / Okta (`/scim/v2`) — see `docs/scim-identity.md`
- Experimental OpenStack Keystone/Nova/Glance/Neutron/Cinder façade — see `docs/openstack-compat.md`

### Authorization (RBAC)

- Admin role (full access)
- User role (read/write)
- Viewer role (read-only)
- Resource-level authorization

### Infrastructure

- TLS/HTTPS support
- Certificate management
- Encryption at rest
- Security middleware with per-endpoint role checks
- Input validation on all user-facing parameters (VM names, IP addresses, file paths, storage identifiers)
- Parameterized SQL queries (no injection risk)
- No shell pipelines — all subprocess calls use direct argument passing
- CIDR validation on network policy rules
- Storage name validation (LVM, ZFS, NFS)

### Audit Logging

- Structured logging with tracing
- Audit logging for all operations with export
- User action tracking
- Resource modification logs
- Security event logging

## Storage

### Backends

- Local filesystem
- NFS
- LVM and LVM-thin
- ZFS (with replication)
- Ceph/RBD (cluster health, stats, RBD image management)
- Distributed storage

### Volume Operations

- Create, delete, resize, clone, attach, detach volumes
- Volume info retrieval
- Multiple formats (qcow2, raw, vmdk, vdi)

### Snapshots

- Create, list, restore snapshots
- Internal qcow2 snapshots

### Backup and Restore

- Full and incremental backups

## Networking

### Network Configuration

- Multiple network interfaces per VM
- Bridge management (create/delete)
- VLAN support
- MAC address generation
- MTU configuration

### Port Forwarding

- TCP and UDP protocol support
- nftables integration
- Forwards bind `0.0.0.0`, so they're reachable from any client on the network, not just from the host itself
- Set at VM creation via the Create VM wizard's "Expose ports" section (with a one-click Expose SSH (22) preset), or added/removed later for a running or stopped VM from its Network tab
- Adding or removing a forward on a running VM restarts it automatically to apply the change (`POST`/`DELETE /api/vms/:name/port-forwards`)

### Network Modes

- NAT mode (default) — private, outbound-only address; reachability limited to explicit port forwards
- Bridged mode — VM gets its own address on the local network via a dedicated network namespace, veth pair, and per-namespace dnsmasq DHCP server, instead of sharing the host's NAT
  - DHCP addressing (default) — the guest leases an address automatically
  - Static addressing — the address is baked in via a generated cloud-init netplan config at boot, for guests that don't run a DHCP client or need a predictable address before first boot
- Isolated mode
- VLAN isolation

### Network Security (Cilium-style)

- **Network Policies** -- Label-based ingress/egress rules with direction badges, priority, and enforcement status
- **VM edge dataplane (FluxVM)** -- Separate from SDN: per-VM TC/eBPF allowlists, groups/CNP, Mbps/PPS, stats/flows via Dataplane tab / `/api/vms/{name}/dataplane/*` and `/api/dataplane/*` when FluxVM runs `mode=ebpf` (schema v4). Ports are `tcp|udp/PORT`. Dashboard capability `vm_dataplane`. Console `/app/edge-dataplane`.
- **VM Firewall** -- Per-VM profiles with rule builder (protocol/port/CIDR/action), zones, and VM assignments
- **Service Mesh** -- Virtual IP services with load balancing (round-robin, least-conn, random, IP-hash), backend management
- **QoS / Traffic Shaping** -- Guaranteed/max rate with burst, priority-based bandwidth management, label selectors
- **DNS Policy** -- Zone management with record types (A/AAAA/CNAME/MX/TXT/SRV/PTR), upstream servers, domain blocking
- **VPN Mesh** -- WireGuard tunnels with peer editor, network topology selector (full-mesh, hub-spoke, point-to-point)
- **Packet Mirror** -- Direction selector (ingress/egress/both), collector target, protocol/port/CIDR filters
- **NAT Gateway** -- Rule types (masquerade/SNAT/DNAT/hairpin), IP pool editor, gateway configuration
- **Network Monitor** -- Threshold builder (metric/value/unit/direction/severity), live metrics, alert management
- Create and edit modals for every resource type above, plus bridges, bonds, and VLANs on the Network page

### VPN Mesh

- WireGuard-based VPN tunnels
- Point-to-point, hub-spoke, and full-mesh topologies
- Auto-mesh via label selectors
- Persistent keepalive

### Packet Mirror

- Traffic mirroring via tc mirred
- Per-VM tap interface mirroring
- Ingress, egress, and bidirectional capture
- Protocol and port filtering

### NAT Gateway

- Masquerade NAT for VM internet access
- SNAT with IP address pools
- DNAT for inbound port forwarding
- Hairpin NAT for internal loopback
- nftables-based enforcement (`zyvor-fabricd_nat` table)

### Network Monitor

- Per-VM bandwidth monitoring
- Real-time rx/tx byte and packet rates
- Threshold-based alerts (bps, kbps, mbps, gbps)
- Alert severities (info, warning, critical)
- Log, event, and webhook alert actions
- sysfs counter collection

## High Availability

### Clustering

- etcd integration
- Multi-node support
- Leader election
- Node registration
- Heartbeat mechanism
- Health monitoring

### Failover

- Automatic leader election
- Node health checks
- Cluster state management
- Distributed configuration

### Fault Tolerance and Recovery

- Fault tolerance
- Replication
- Site recovery

## Resource Management

- Resource quotas
- Resource pools
- Autoscaling
- Performance analytics
- Predictive DRS (4 algorithms)
- Datacenters management
- CPU pinning (Auto, Explicit vCPU-to-pCPU map, NUMA node, Socket) and NUMA/CPU topology inspection, from the VM's Advanced tab

## Notifications

- Email
- Slack
- Webhook
- Microsoft Teams

## Content and Image Management

- Content library
- Image builder

## Host Agent

- Per-host agent for distributed management

## Ecosystem Integration

### Kubernetes Operator

- Custom Resource Definition (CRD)
- Controller implementation
- Status reporting and event handling
- Helm charts
- Automatic reconciliation
- cloud-init, TPM, and VNC integration

### Terraform Provider

- Resources: `zyvor-fabricd_vm`, `zyvor-fabricd_storage_pool`, `zyvor-fabricd_network_policy`, `zyvor-fabricd_vm_snapshot`
- Data source: `zyvor-fabricd_vm`
- Example configurations

### Monitoring

- Prometheus metrics (`/metrics` endpoint)
- VM count and state metrics
- Operation counters (starts, stops, creates, deletes)
- Grafana dashboard and alert rules

## REST API (780+ Endpoints)

### WebSocket Endpoints

- `WS /ws/console/:name` -- Console
- `WS /ws/vnc/:name` -- VNC
- `WS /ws/events` -- Events

### Examples

- `GET /api/vms` -- List VMs
- `POST /api/vms` -- Create VM
- `GET /api/vms/:name` -- Get VM details
- `DELETE /api/vms/:name` -- Delete VM
- `POST /api/vms/:name/start` -- Start VM
- `POST /api/vms/:name/stop` -- Stop VM
- `POST /api/vms/:name/restart` -- Restart VM
- `GET /api/vms/:name/metrics` -- Get metrics
- `POST /api/vms/:name/cloud-init` -- Configure cloud-init
- `GET /api/cluster/nodes` -- List cluster nodes
- `GET /api/cluster/leader` -- Get current leader
- `GET /metrics` -- Prometheus metrics
- `GET /health` -- Health check

### OpenStack Compatibility (experimental)

Same daemon port as Fabric (default **9095**). Catalog uses `daemon.public_url` /
`ZYVOR_FABRICD_PUBLIC_URL`. See `docs/openstack-compat.md` and
**Tutorial 08** (`docs/tutorials/08-openstack-clients.md`).

- `POST /identity/v3/auth/tokens` -- Keystone token
- `GET /compute/v2.1/flavors` -- Nova flavors
- `POST /compute/v2.1/servers` -- Create server
- `GET /image/v2/images` -- Glance images
- `GET /network/v2.0/networks` -- Neutron networks
- `POST /volume/v3/{project_id}/volumes` -- Cinder volumes

## Deployment

### Installation

- Build from source
- Optional systemd service (not required — the daemon runs fine under any supervisor, or in the foreground)
- Make install script
- Remote bare-metal: `./scripts/deploy remote USER@HOST`
- **Kubernetes platform** (fabricd + FluxVM DaemonSets, Helm): see `docs/KUBERNETES.md` / `./scripts/deploy k8s USER@HOST`
- Helm chart for the **operator** (CRDs → Fabric API)

### Configuration

- TOML configuration file
- Environment variables
- Command-line arguments
- Runtime configuration

## Build and Development

- Cargo workspace (53 crates)
- npm/Vite for web UI
- GitHub Actions CI/CD
- Automated builds, formatting checks, linting
- Makefile for common tasks (`k8s-deploy`, `helm-lint`, …)
- Docker and Podman support -- `zyvor-fabricd` + FluxVM companion, see `docs/DOCKER.md`
- Kubernetes manifests under `k8s/base/` and chart under `charts/zyvor-fabric/`

## Keep (agent-runtime)

- Soft Keep 0.2: vault ceremony, screencast, user-held challenge scaffold, fabricd proxies
- **Brokered browser:** Chromium CDP `:9222` + a11y driver `:9230` (`browser-agent` template)
- **Keep Browser 0.3:** split-sight pause, trajectory-as-code, origin IFC, SNI-identity, goal tabs, honesty badge
- **Keep host eBPF (FluxVM):** `deny_udp`, gateway-only L4, cockpit `egress_connects` / `drop_reasons`, `ebpf_deny` freeze — no PacketWolf dependency
- **PDF brief one-click demo:** `/app/keep` → Brief this PDF; `POST /api/demos/pdf-brief`; expect 0 CONNECT
- MCP tools `browser_open|snapshot|act|tabs|close` (no HTML/DOM/evaluate to the model)
- Host `POST …/browser/fill-secret` after vault `authorize_resolve` (password never in transcript)
- Policy `browser:` block in `merion.policy.yaml` (allow_hosts, file:// deny, downloads deny, confinement:strict gate)
- Operator: `merionctl browser|session|browse`, MerionSession capability + honesty badge
- Pack: `examples/merion-agents/browser-research/`
- Docs: [DRIVER.md](docs/merion/browser/DRIVER.md), [BROWSER-0.3.md](docs/merion/browser/BROWSER-0.3.md)
- Evidence class remains **software-test** until SNP/TDX (hardware Keep 0.2 gated)
