# Zyvor Fabric Documentation Index

Complete documentation index for the Zyvor Fabric infrastructure control plane (`Zyvor Fabric` daemon).

---

## Getting Started

Guides for new users to install, configure, and begin using Zyvor Fabric.

| Document | Description |
|----------|-------------|
| [Getting Started Overview](getting-started/README.md) | Section overview and reading order |
| [Installation Guide](getting-started/01-Installation.md) | System requirements, packages, building from source |
| [Quick Start](getting-started/02-Quick-Start.md) | Create your first VM in 5 minutes |
| [Configuration Reference](getting-started/03-Configuration.md) | Config file, environment variables, all sections |
| [Web UI Guide](getting-started/04-Web-UI.md) | Dashboard access, login, VM management through browser |

---

## Tutorials

Step-by-step walkthroughs for common workflows.

| Document | Description |
|----------|-------------|
| [Tutorial index](tutorials/README.md) | Full numbered tutorial list |
| [01 First VM](tutorials/01-first-vm.md) | Create, configure, start, and connect to a virtual machine |
| [02 Networking](tutorials/02-networking.md) | Bridges, VLANs, bonds, port forwarding, network policies |
| [03 Snapshots & Backups](tutorials/03-snapshots-backups.md) | Snapshots, backup policies, restore |
| [04 Advanced VM options](tutorials/04-advanced-vm-options.md) | Hotplug, start options, cloud-init |
| [05 Clustering](tutorials/05-clustering.md) | Datacenters, pools, migration, HA (see also proven-infra limits) |
| [06 Security hardening](tutorials/06-security-hardening.md) | Auth, RBAC, TLS, audit |
| [07 Logging & compliance](tutorials/07-logging-compliance.md) | Journals, scans, secrets |
| [08 OpenStack Clients](tutorials/08-openstack-clients.md) | Drive Fabric with `openstack` CLI / curl (Keystone/Nova/…) |
| [09 Edge dataplane](tutorials/09-edge-dataplane.md) | FluxVM Network Fabric via Fabric API / CLI / UI |
| [15 AI Workloads](tutorials/15-ai-workloads.md) | OpenAI inference: Janus or NVIDIA, gateway, MIG (Beta) |
| [16 Keep workstation](tutorials/16-keep-workstation.md) | Keep mode, signed policy, cockpit, browser |
| [17 Keep PDF brief](tutorials/17-keep-pdf-brief.md) | One-click PDF → brief.md, 0 CONNECT |
| [OpenStack Compatibility reference](openstack-compat.md) | Endpoints, public URL, limitations |
| [Proven infrastructure](proven-infra/README.md) | Compatibility matrix, SLOs, chaos, upgrade/rollback (#14–#17) |

---

## Guides

### CLI Guides

| Document | Description |
|----------|-------------|
| fabricctl Reference | Full CLI command reference with examples |
| zyvor-fabricd-ctl Operations | Deployment, management, and maintenance commands |
| Declarative Configuration | Define VM infrastructure as YAML with `fabricctl apply` |
| Shell Completions | Enable tab completion for bash |

### Operations Guides

| Document | Description |
|----------|-------------|
| Production Deployment | Hardening, TLS, monitoring, and logging for production |
| Capacity Planning | Resource estimation and scaling guidelines |
| Backup Strategy | Backup types, retention policies, and disaster recovery |
| Monitoring Setup | Prometheus integration, alerting, and dashboards |
| Troubleshooting | Common issues, log analysis, and diagnostic steps |
| Upgrade Procedures | Rolling upgrades and version migration |
| Security Hardening | Firewall rules, RBAC policies, and credential rotation |

### Decision Support

| Document | Description |
|----------|-------------|
| Zyvor Fabric vs. Proxmox | Feature and architecture comparison |
| Zyvor Fabric vs. OpenStack | Scope, complexity, and deployment comparison |
| Zyvor Fabric vs. libvirt | Integration model and management layer comparison |
| Storage Backend Selection | Choosing between Local, NFS, LVM, ZFS, and Ceph |
| Networking Architecture | Overlay vs. flat networking decisions |

---

## Features

Detailed documentation for each major feature area.

### VM Lifecycle

| Document | Description |
|----------|-------------|
| VM Creation | `CreateVMRequest` fields, validation rules, defaults |
| VM Start Options | `VMStartOptions` reference -- KVM, TPM, Secure Boot, networking |
| VM States | State machine: Stopped, Starting, Running, Paused, Stopping, Failed |
| VM Cloning | Full and linked clones with CoW support |
| VM Templates | Create, manage, and deploy from templates |
| VM Profiles | Instance types and resource presets |
| VM Import | Import from VMDK, VDI, VHD formats |
| OVA/OVF Export | Export VMs to OVA/OVF format for portability |
| Declarative Specs | YAML-based VM definitions with `fabricctl apply` |
| VM Checkpoints | Create and restore in-memory checkpoints |
| VM Forking | Fork a running VM for testing |

### Container Workloads

| Document | Description |
|----------|-------------|
| [Container Groups](container-groups.md) | Kubernetes-style Pod groups backed by FluxVM Secure Containers -- tenant scoping, image pull secrets, health probes, NetworkPolicy, quotas/billing, backup/restore |

### Storage

| Document | Description |
|----------|-------------|
| [Storage Overview](storage.md) | Storage architecture and backend selection |
| Local Storage | File-based qcow2/raw storage pools |
| NFS Storage | [NFS Storage Guide](NFS_STORAGE_GUIDE.md) |
| LVM Storage | Logical Volume Manager pools and thin provisioning |
| ZFS Storage | ZFS pools, datasets, and replication |
| Ceph/RBD Storage | Ceph cluster integration and RBD image management |
| Volume Management | CRUD, attach/detach, resize, and clone volumes |
| Snapshots | Create, revert, delete, and tree-view snapshots |
| Distributed Storage | Cross-node storage pools, migrations, and policies |
| Datastore Clusters | Storage DRS and placement recommendations |
| iSCSI Storage | iSCSI target discovery, login, and LUN management |
| Cloud Images | Built-in catalog download (Ubuntu, Fedora, Debian, Alma) |
| ISO Management | Download, list, and manage ISO images |
| Online Disk Resize | Grow VM disks without downtime |

### Networking

| Document | Description |
|----------|-------------|
| [Networking Overview](networking.md) | Network architecture, SDN stack, and VM edge dataplane |
| [VM edge dataplane (Network Fabric schema v4)](guides/vm-drivers/fluxvm-dataplane.md) | Fabric proxy of FluxVM TC/eBPF — API, Web, CLI, lab UX |
| [Service Fabric v6 (Maglev VIP LB)](ebpf-service-fabric.md) | FluxVM Maglev/NAT/DSR/EDT/flows/policy + Fabric leases, HA, FRR/BIRD |
| [Where it fits](use-cases.md) | Which part to use for agent sandboxes, a private cloud, Kubernetes, Macs and inference |
| [AI Workloads (Beta)](ai-workloads.md) | GPU VMs, Janus lab upstream, PCI MIG flag, admit webhook, fabric-inference JWT, revisioned rollouts, gateway limits, Maglev, Terraform, operator CRDs |
| [User: VM Dataplane](user/pages/infrastructure/dataplane.md) | Console walkthrough for Status / Policy / Effective / Stats / Flows |
| [User: Edge Dataplane](user/pages/infrastructure/edge-dataplane.md) | Cluster groups / CNP / Maglev Services / health / observe console |
| [Tutorial 09: Edge Dataplane](tutorials/09-edge-dataplane.md) | End-to-end lab (API + CLI + UX) |
| [Edge dataplane series](tutorials/edge-dataplane/README.md) | Short labs: groups, CNP, FQDN, UX |
| Networking (netlink) | Bridges, VLANs, bonds, taps, macvtaps, VXLANs, SR-IOV |
| Network Policies | Cilium-style label-based ingress/egress rules |
| VM Firewall | Per-VM firewall profiles and zones via nftables |
| Service Mesh | Virtual IP load balancing (round-robin, least-conn, IP-hash) |
| Traffic Shaping | QoS: guaranteed rate, max rate, burst, priority-based bandwidth |
| DNS Policies | Zone management, upstream servers, domain blocking |
| VPN Mesh | WireGuard tunnels: point-to-point, hub-spoke, full-mesh |
| NAT Gateway | Masquerade, SNAT, DNAT, hairpin NAT |
| Packet Mirror | Traffic capture and debugging |
| Network Monitor | Per-VM bandwidth tracking with threshold alerts |
| Floating IPs | Virtual IP allocation and VM assignment |
| DHCP Servers | Per-bridge dnsmasq-managed DHCP servers |
| DHCP Server | Built-in DHCP server on bridge interfaces |
| Port Forwarding | NAT-based port forwarding rules |

### Security and Identity

| Document | Description |
|----------|-------------|
| [Security Overview](security.md) | Security architecture and audit history |
| Authentication | PAM + JWT authentication flow |
| RBAC | Admin, User, Viewer roles and endpoint permissions |
| API Keys | Service-to-service authentication tokens |
| [OIDC / External Auth](oidc.md) | LDAP and OIDC/OAuth2 SSO (PKCE, JWKS-verified id_token) |
| [SCIM Provisioning](scim-identity.md) | SCIM 2.0 lifecycle provisioning and group-to-role sync for Entra ID / Okta |
| [OpenStack Compatibility](openstack-compat.md) | Experimental Keystone/Nova/Glance/Neutron/Cinder façade on the same daemon port |
| Audit Logging | Structured audit logs with JSON/CSV export |
| Encryption | VM disk encryption with key management providers |
| TLS/HTTPS | Certificate management and self-signed TLS generation |
| Multi-Tenancy | Project isolation with member roles and quotas |
| 2FA/MFA Authentication | TOTP-based two-factor authentication for user accounts |
| Secrets Management | Secure storage and retrieval of credentials and sensitive data |
| Compliance Scanning | Automated compliance profile scanning and reporting |
| JWT Revocation | Per-token revocation via JTI blocklist |
| Credential Management | systemd credentials, SSH keys, and cloud-init secrets |

### High Availability

| Document | Description |
|----------|-------------|
| DRS | Distributed Resource Scheduling and placement |
| Affinity Rules | VM-to-VM and VM-to-host affinity/anti-affinity |
| [Host Maintenance Evacuation](host-lifecycle.md) | Preflight-checked workload evacuation before a host enters maintenance |
| [Fabric Doctor](FABRIC_DOCTOR.md) | Production host preflight checks and safe support bundles (`tools/fabric-doctor`) |
| Fault Tolerance | Automatic failover, fencing, and recovery |
| Live Migration | Disk-copy: iterative rsync pre-copy + cutover; native FluxVM transport = preview (receivers) |
| Site Recovery | Failover/reprotect workflows for disaster recovery |
| Resource Overcommit | CPU/memory/storage overcommit policies |
| Split-Brain Protection | Quorum-based fencing to prevent split-brain in clusters |
| Auto-Scaling | Metric-based scaling policies and events |

### Monitoring and Automation

| Document | Description |
|----------|-------------|
| Prometheus Metrics | `/metrics` endpoint and metric catalog |
| Analytics Dashboard | Historical performance data and insights |
| Notifications | Email, Slack, Webhook, Microsoft Teams channels |
| Schedules | Once, daily, weekly VM operations scheduling |
| Backup Automation | Retention policies, incremental backups, systemd timers |
| Resource Quotas | Per-user and per-project resource limits |
| Optimization | Resource optimization recommendations |
| Log Aggregation | Centralized VM log collection, search, and streaming |
| Health Checks | Deep health check: API, disk, DB, credentials, KVM |

### Console Access

| Document | Description |
|----------|-------------|
| WebSocket Console | Browser-based terminal via xterm.js |
| VNC Console | Graphical console via noVNC proxy |
| SPICE Display | SPICE protocol support for high-performance remote display |
| Console Modes | Interactive, read-only, native, GUI |

### Advanced Virtualization

| Document | Description |
|----------|-------------|
| [GPU Passthrough](gpu-passthrough.md) | Generic PCI/VFIO passthrough for NVIDIA and AMD GPUs (no vGPU/Intel GVT-g) |
| [CPU/NUMA Optimization](CPU_NUMA_OPTIMIZATION_GUIDE.md) | CPU pinning, NUMA topology, hugepages |
| KSM Memory Dedup | Kernel Same-page Merging configuration |
| Nested Virtualization | Running VMs inside VMs |
| Hotplug | CPU, memory, disk, and NIC hot-add/remove |
| Firmware | UEFI, Secure Boot, NVRAM management |
| TPM | vTPM support via swtpm |
| Direct Kernel Boot | Boot from kernel + initrd without bootloader |
| Bind Mounts | Host-to-VM filesystem sharing |
| USB Passthrough | Host USB device passthrough to VMs |
| User Namespaces | Private user mapping for isolation |

---

## Architecture

| Document | Description |
|----------|-------------|
| [Architecture Overview](architecture.md) | System architecture and component diagram |
| Crate Structure | 53 backend crates and their responsibilities |
| Data Model | VM, VMStartOptions, VMMetrics, VMPressure |
| Driver Model | VMDriver and ResourceStatsDriver traits |
| State Store | SQLite-based persistent state management |
| Event System | Broadcast channels and SSE event streaming |
| Background Tasks | Reconciliation loops and schedulers |
| Plugin System | Plugin registry and extension points |

---

## Reference

### API Reference

| Document | Description |
|----------|-------------|
| [API Overview](api.md) | Authentication, pagination, error format |
| VM Endpoints | CRUD, lifecycle, metrics, cloud-init |
| Storage Endpoints | Pool and volume management |
| Network Endpoints | Bridges/VLANs/bonds via netlink, policies, firewall, mesh |
| Security Endpoints | Auth, audit, encryption, tenants |
| System Endpoints | CPU topology, NUMA, memory, firmware |
| Enterprise Endpoints | Datacenters, clusters, hosts, DRS |
| Machine Endpoints | VM driver: FluxVM (no systemd dependency) |
| Monitoring Endpoints | Analytics, events, notifications, schedules |
| Backup Endpoints | Backup CRUD, policies, restore |
| Billing Endpoints | Usage tracking, pricing, and invoicing |
| WebSocket Endpoints | Console, VNC, event streaming |

### CLI Reference

| Command | Description |
|---------|-------------|
| `fabricctl` / `fabricctl --help` | Cilium-style grouped help with emoji section markers (Basic, Dataplane, Networking, Meta, …) |
| `fabricctl list` | List VMs (paginated `{items, total, …}` from `GET /api/vms`); set `FABRIC_URL` + `FABRIC_TOKEN` or `ZYVOR_FABRIC_*` for HTTPS labs |
| `fabricctl create` | Create a new VM |
| `fabricctl start` | Start a stopped VM |
| `fabricctl stop` | Stop a running VM |
| `fabricctl restart` | Restart a VM |
| `fabricctl delete` | Delete a VM |
| `fabricctl apply` | Apply declarative YAML specification |
| `fabricctl policy` | Manage network policies |
| `fabricctl dataplane` | VM edge dataplane (policy, flows, hubble, CNP, …) |
| `fabricctl status` | Fabric API + dataplane health checklist |
| `fabricctl config` | Show effective server/token/color settings |
| `fabricctl completion` | Generate bash/zsh/fish completion scripts |
| `fabricctl ceph` | Ceph storage management |
| `fabricctl metrics` | Get VM metrics |

Global flags: `-o table|json|yaml`, `--color auto|always|never`, `--server`, `--token`.

VM console/VNC access is Web/REST-only (`GET /ws/console/:name`, `/ws/vnc/:name`) — there is no `fabricctl console` command.

### zyvor-fabricd-ctl Reference

| Command | Description |
|---------|-------------|
| `zyvor-fabricd-ctl deploy` | Full deployment (deps + build + install + start) |
| `zyvor-fabricd-ctl deps` | Install system dependencies |
| `zyvor-fabricd-ctl build` | Build from source |
| `zyvor-fabricd-ctl install` | Install binaries and systemd units |
| `zyvor-fabricd-ctl start` | Start the zyvor-fabricd service |
| `zyvor-fabricd-ctl stop` | Stop the zyvor-fabricd service |
| `zyvor-fabricd-ctl restart` | Restart the service |
| `zyvor-fabricd-ctl status` | Show service status |
| `zyvor-fabricd-ctl logs` | Follow service logs |
| `zyvor-fabricd-ctl verify` | Post-install smoke test |
| `zyvor-fabricd-ctl health` | Deep health check |
| `zyvor-fabricd-ctl password` | Read the admin password |
| `zyvor-fabricd-ctl doctor` | System readiness check |
| `zyvor-fabricd-ctl tls` | Generate self-signed TLS certificate |
| `zyvor-fabricd-ctl upgrade` | Git pull + reinstall |
| `zyvor-fabricd-ctl uninstall` | Remove everything |
| `zyvor-fabricd-ctl billing` | View billing and usage reports |
| `zyvor-fabricd-ctl backup now` | Trigger immediate backup |
| `zyvor-fabricd-ctl backup enable` | Enable daily backup timer |
| `zyvor-fabricd-ctl backup status` | Show backup timer and storage info |

### Rust SDK

| Item | Description |
|------|-------------|
| `zyvor-fabric-sdk` | Typed Rust SDK for the Zyvor Fabric API with async client |
| Authentication | Login, token refresh, and 2FA helpers |
| VM Operations | Create, start, stop, delete, clone, and snapshot VMs |
| Storage / Networking | Pool, volume, bridge, VLAN, and firewall management |
| Streaming | WebSocket console attach and SSE event subscriptions |

---

## Deployment

| Document | Description |
|----------|-------------|
| **[Kubernetes](KUBERNETES.md)** | Run fabricd + FluxVM as privileged DaemonSets; Helm; lab `./scripts/deploy k8s` |
| [Docker / Podman](DOCKER.md) | Local eval with compose (`hostNetwork` + KVM) |
| Single Server | One-node deployment for development and small teams |
| Multi-Node Cluster | HA deployment with shared storage and etcd |
| Kubernetes Operator | VMs as CRDs with the zyvor-fabricd operator ([operator/](../operator/)) |
| Terraform Provider | Declarative provisioning with plan/apply |
| Edge Deployment | Lightweight single-node deployment for edge locations |
| Air-Gapped Install | Offline installation without internet access |

---

## Development

| Document | Description |
|----------|-------------|
| Development Setup | Rust toolchain, IDE, and local development |
| Build and Test | `cargo check`, `cargo test`, CI pipeline |
| Crate Map | 53 crates and their dependencies |
| Adding an API Endpoint | Step-by-step guide for new endpoints |
| Adding a Storage Backend | Driver trait implementation guide |
| Adding a Network Feature | Integration with the netlink-based networking crate |
| Code Style | Formatting, naming, error handling conventions |
| Security Guidelines | Input validation, path traversal prevention, audit |

---

## Quick Reference

See also the standalone quick-reference docs: **[FAQ](quick-reference/faq.md)** · [Glossary](quick-reference/glossary.md) · [Quick reference cheatsheet](quick-reference/quick-reference.md).

### API Endpoint Categories

The REST API is organized into the following endpoint groups:

| Category | Prefix | Endpoints | Description |
|----------|--------|-----------|-------------|
| Authentication | `/api/auth/` | 6 | Login, 2FA/TOTP setup, and session management |
| SCIM Identity | `/api/identity/scim/`, `/scim/v2/` | 21 | Provisioning profiles/tokens (JWT) plus SCIM Users/Groups (bearer token) |
| OpenStack Compatibility | `/identity`, `/compute`, `/image`, `/network`, `/volume` | — | Experimental OpenStack wire protocol (catalog from `public_url`) |
| VM Lifecycle | `/api/vms/` | 12 | CRUD, start, stop, restart, pause, resume, clone |
| VM Advanced | `/api/vms/{name}/` | 20+ | Hotplug, checkpoints, fork, disk resize, firmware |
| Snapshots | `/api/vms/{name}/snapshots/` | 5 | Create, list, get, delete, revert |
| Cloud-Init | `/api/vms/{name}/cloud-init` | 1 | Configure cloud-init for a VM |
| Storage Pools | `/api/storage/pools/` | 14 | Pool CRUD, health, stats, refresh |
| Volumes | `/api/storage/pools/{name}/volumes/` | 6 | Volume CRUD, resize, attach, detach |
| Distributed Storage | `/api/distributed-storage/` | 18 | Cross-node pools, migrations, policies |
| Networking | `/api/networkd/` | 35+ | Bridges, VLANs, bonds, taps, VXLANs, SR-IOV (netlink-based) |
| Network Policies | `/api/network-policies/` | 5 | Cilium-style ingress/egress rules |
| VM Firewall | `/api/vm-firewall/` | 8+ | Per-VM firewall profiles and zones |
| Service Mesh | `/api/service-mesh/` | 10+ | Virtual IP and load balancing |
| Traffic Shaping | `/api/traffic-shaping/` | 8+ | QoS and bandwidth management |
| DNS Policies | `/api/dns-policies/` | 6+ | Zone management and blocking |
| VPN Mesh | `/api/vpn-mesh/` | 6+ | WireGuard tunnel management |
| NAT Gateway | `/api/nat-gateway/` | 6+ | Masquerade, SNAT, DNAT |
| Packet Mirror | `/api/packet-mirror/` | 4+ | Traffic capture |
| Net Monitor | `/api/net-monitor/` | 4+ | Bandwidth tracking and alerts |
| Floating IPs | `/api/floating-ips/` | 4 | Virtual IP allocation |
| DHCP | `/api/dhcp-servers/` | 2 | Per-bridge dnsmasq-managed DHCP servers |
| DNS | `/api/dns/` | 4 | DNS configuration |
| Encryption | `/api/encryption/` | 11 | Key providers, policies, VM encryption |
| Backups | `/api/backups/` | 11 | Backup CRUD, policies, restore, stats |
| Schedules | `/api/schedules/` | 9 | Timed VM operations |
| Quotas | `/api/quotas/` | 8 | Resource quotas and usage |
| Notifications | `/api/notifications/` | 11 | Multi-channel alerting |
| Audit | `/api/audit/` | 4 | Audit logs and export |
| Analytics | `/api/analytics/` | 6 | Performance data and insights |
| Templates | `/api/templates/` | 5 | VM templates and deployment |
| Profiles | `/api/profiles/` | 3 | Instance type presets |
| Images | `/api/images/` | 10 | Image build, cloud download, ISO, import |
| Migrations | `/api/migrations/` | 3 | Live VM migration |
| Events | `/api/events/` | 2 | Event list and SSE stream |
| System | `/api/system/` | 12 | CPU, NUMA, memory, hugepages, optimization |
| Firmware | `/api/vms/{name}/firmware/` | 5 | UEFI, Secure Boot, NVRAM |
| Datacenters | `/api/datacenters/` | 5 | Datacenter management |
| Clusters | `/api/clusters/` | 5 | Cluster management and health |
| Hosts | `/api/hosts/` | 8 | Host registration and maintenance |
| DRS | `/api/drs/` | 9 | Scheduling, placement, affinity rules |
| Resource Pools | `/api/resource-pools/` | 7 | Resource pool management |
| Zones | `/api/zones/` | 3 | Availability zones |
| Spot Instances | `/api/spot-instances/` | 3 | Spot VM management |
| Tenants | `/api/tenants/` | varies | Multi-tenancy and projects |
| Settings | `/api/settings` | 2 | Global settings |
| Plugins | `/api/plugins` | 1 | Plugin registry |
| OVA Export | `/api/vms/{name}/export/` | 2 | OVA/OVF export |
| Secrets | `/api/secrets/` | 5 | Secrets and credential management |
| Compliance | `/api/compliance/` | 6 | Compliance profile scanning and results |
| Billing | `/api/billing/` | 6 | Usage tracking, pricing, and invoicing |
| Logs | `/api/logs/` | 4 | Centralized log aggregation and search |
| iSCSI | `/api/iscsi/` | 5 | iSCSI target discovery and session management |
| USB | `/api/usb/` | 4 | USB device listing and passthrough |
| SPICE | `/api/vms/{name}/spice/` | 2 | SPICE display connection |
| Declarative | `/api/vms/apply` | 2 | YAML spec apply and export |
| Auto-Scale | `/api/autoscale/` | 4 | Scaling policies and events |
| WebSocket | `/api/ws/` | 3 | Console, VNC, events |

### Default Ports and Paths

| Item | Default |
|------|---------|
| API listen address | `127.0.0.1:9095` |
| Config file | `/etc/zyvor-fabricd/zyvor-fabricd.toml` |
| State directory | `/var/lib/zyvor-fabricd/` |
| Image directory | `/var/lib/zyvor-fabricd/images/` |
| Auth database | `/var/lib/zyvor-fabricd/auth.db` |
| Cloud-init directory | `/var/lib/zyvor-fabricd/cloud-init/` |
| Storage pools | `/var/lib/zyvor-fabricd/storage/` |
| JWT secret file | `/var/lib/zyvor-fabricd/.jwt_secret` |
| Admin password file | `/var/lib/zyvor-fabricd/.admin_password` |
| networkd config dir | `/etc/systemd/network/` |
| networkd file prefix | `50-Zyvor Fabric-` |
| Network bridge | `br0` |

### VM Resource Limits

| Resource | Minimum | Maximum | Default |
|----------|---------|---------|---------|
| CPUs | 1 | 256 | -- |
| Memory | 128 MB | 1,048,576 MB (1 TB) | -- |
| Disk | 1 GB | 65,536 GB (64 TB) | 20 GB |

### VM States

| State | Description |
|-------|-------------|
| `stopped` | VM is not running |
| `starting` | VM is in the process of booting |
| `running` | VM is running and accessible |
| `paused` | VM is suspended in memory |
| `stopping` | VM is in the process of shutting down |
| `failed` | VM failed to start or encountered an error |
| `unknown` | VM state could not be determined |

### RBAC Roles

| Role | Read | Write | Admin |
|------|------|-------|-------|
| Viewer | Yes | No | No |
| User | Yes | Yes | No |
| Admin | Yes | Yes | Yes |

### Environment Variables

| Variable | Description |
|----------|-------------|
| `ZYVOR_FABRICD_LISTEN` | Override `daemon.listen` (e.g. `0.0.0.0:9095`) |
| `ZYVOR_FABRICD_PUBLIC_URL` | External base URL for OpenStack catalog / clients |
| `ZYVOR_FABRICD_CONFIG` | Override config file path |
| `ZYVOR_FABRICD_LOG_LEVEL` | Override log level |
| `ZYVOR_FABRICD_JWT_SECRET` | Override JWT signing secret |
| `ZYVOR_FABRICD_ADMIN_PASSWORD` | Override default admin password |
| `ZYVOR_FABRICD_BACKUP_DIR` | Override backup directory |
| `ZYVOR_FABRICD_BACKUP_RETAIN` | Override backup retention count |
| `ZYVOR_FABRICD_BACKUP_TYPE` | Override backup type |

---

## AI Workloads (Beta)

OpenAI-compatible inference on dedicated NVIDIA GPU VMs: ModelArtifact /
InferenceProfile / InferenceDeployment / InferenceEndpoint, revisioned
rollouts, Maglev weighting, gateway rate limits, an API-key (or
`fabric-inference` JWT) OpenAI gateway at `/api/ai/openai/{endpoint}`, and an
optional Janus stand-in when the host has no NVIDIA GPU. Console: `/app/ai`.
**Single-cluster Beta** (not GA). Multi-site HA store stays Preview.

| Document | Description |
|----------|-------------|
| [ai-workloads.md](ai-workloads.md) | Full reference: REST, CLI, Janus, PCI MIG, admit, Raft, runtimes |
| [Tutorial 15](tutorials/15-ai-workloads.md) | Operator how-to: model → chat → MIG |
| [scripts/bake-ai-vllm-image.sh](../scripts/bake-ai-vllm-image.sh) | Bake CUDA + vLLM qcow2 |
| [scripts/smoke-ai-janus-lab.sh](../scripts/smoke-ai-janus-lab.sh) | Lab smoke: health, chat, MIG, admit |
| [terraform-provider/examples/ai-workloads](../terraform-provider/examples/ai-workloads/) | Terraform model/profile/deployment/endpoint |
| [operator/examples/ai-inference-deployment.yaml](../operator/examples/ai-inference-deployment.yaml) | ModelArtifact + InferenceDeployment CRs |
| [operator chart admissionWebhook](../operator/charts/zyvor-fabricd-operator/values.yaml) | Opt-in ValidatingWebhook for InferenceDeployment |
| [Agent Runtime ↔ Fabric](../agent-runtime/README.md#fabric-managed-inference-phase-6) | `kind: fabric` credentials for on-prem models |

---

## Agent Runtime

Standalone component: durable agent sessions in a FluxVM sandbox, or on a GitHub runner through a FluxVM stand-in. A session runs a JavaScript worker or a `claude`, `codex`, or `gemini` CLI. Provider keys stay on the host egress broker. The crate also accepts cron schedules, signed webhooks, bounded loops, operator approvals, agent-to-agent delegation, and a small MCP server. Lives in `agent-runtime/` (Rust daemon), `sdk/agent-runtime/` (SDK + `fabric-agent` CLI), and `examples/agent-runtime/` — outside the `backend/` Cargo workspace.

| Document | Description |
|----------|-------------|
| [agent-runtime/README.md](../agent-runtime/README.md) | Deploy/run, harness, schedules, MCP, HTTP API, warm pools, session CI |
| [Tutorial 11](tutorials/11-agent-runtime-quickstart.md) | Build a Node.js FluxVM template and drive a session |
| [Tutorial 13](tutorials/13-deploy-agent-from-console.md) | Build a bundle and deploy it from the web console |
| [Tutorial 14](tutorials/14-interacting-with-agents.md) | Use cases: ops, coding, research, cron, webhook, loop, approval, handoff, IDE chat, fan-out |
| [examples/agent-runtime/README.md](../examples/agent-runtime/README.md) | Ops health check, hello-go, warm pool, and fan-out |

---

## Integrations

- [integrations/README.md](integrations/README.md) — Machina, Terraform, operator, Ansible, OpenStack-compat
- [openstack-compat.md](openstack-compat.md) — drive Fabric with `openstack` / Terraform OpenStack / Ansible `os_*`
- [DEVOPS.md](DEVOPS.md) — CI/CD, GitOps, Terraform, probes, promotion with FluxVM
- [examples/devops/](../examples/devops/README.md) — copy-paste pipelines and gates
- [NAMING.md](NAMING.md) — Clone URL and product naming

## Product Positioning

- [docs/POSITIONING.md](POSITIONING.md) — Zyvor Fabric vs. Machina, naming model, messaging
- [client-presentations/](client-presentations/) — Client-facing HTML slide decks

---

## Client Presentations

| Document | Description |
|----------|-------------|
| [Product Overview](PRODUCT_OVERVIEW.md) | Comprehensive product overview with feature matrix |
| [Product Overview (PDF)](PRODUCT_OVERVIEW.pdf) | Printable product overview |
| [Security Audit Report](SECURITY_AUDIT_REPORT.md) | Full security audit report |
| [Security Audit Report (PDF)](SECURITY_AUDIT_REPORT.pdf) | Printable security audit report |

---

*This index is maintained alongside the codebase. For the latest information, refer to the source code and inline documentation.*
