# Comparison Matrix

Feature-by-feature comparison of Zyvor Fabric against alternative VM management platforms: libvirt/virsh, Proxmox VE, and other tools.

## Table of Contents

- [Architecture Comparison](#architecture-comparison)
- [API and Automation](#api-and-automation)
- [VM Lifecycle](#vm-lifecycle)
- [Networking](#networking)
- [Storage](#storage)
- [Security](#security)
- [Monitoring and Observability](#monitoring-and-observability)
- [Operations](#operations)
- [Summary](#summary)

---

## Architecture Comparison

| Aspect | Zyvor Fabric | libvirt/virsh | Proxmox VE |
|--------|---------|---------------|------------|
| **Hypervisor** | QEMU/KVM/Cloud Hypervisor/Firecracker, via [FluxVM](https://github.com/zyvorai/fluxvm) | QEMU/KVM (+ Xen, LXC) | QEMU/KVM + LXC |
| **Management layer** | FluxVM (systemd-free) + REST API | libvirtd daemon + XML API | pvemanager + Perl API |
| **Configuration format** | JSON (REST API) | XML domain definitions | Perl config files + web UI |
| **Init system integration** | No systemd dependency — direct netlink for networking, FluxVM-supervised VM processes; the daemon's own packaging/init doesn't require systemd either | Standalone (systemd unit for libvirtd only) | Standalone (custom init scripts) |
| **Language** | Rust (backend), React (frontend) | C (libvirt), various (bindings) | Perl (backend), JavaScript (frontend) |
| **Cluster support** | Single-host (multi-host networking available) | Single-host (requires external orchestration) | Built-in multi-node cluster |
| **License** | Zyvor Production 1.0 (free non-production) | LGPL | AGPL (open source) + subscription |

### Key Architectural Differences

**Zyvor Fabric** exposes VM lifecycle through one REST API backed by [FluxVM](https://github.com/zyvorai/fluxvm) -- a disposable-VM engine with no systemd dependency of its own. FluxVM supervises each VM's QEMU / Cloud Hypervisor / Firecracker / FluxVM hypervisor process directly (never as a systemd unit); the API surface covers lifecycle, cgroup resource control, log streaming, hotplug, and image management.

**libvirt** provides its own abstraction layer over hypervisors. It manages VM lifecycle through its own daemon (libvirtd) and stores configuration as XML. VMs are tracked internally by libvirt, independent of any init system.

**Proxmox VE** is a full-stack solution that includes its own cluster filesystem (pmxcfs), authentication (PAM + PVE realm), and web UI. It uses qemu-server for VM management with its own configuration format.

---

## API and Automation

| Feature | Zyvor Fabric | libvirt/virsh | Proxmox VE |
|---------|---------|---------------|------------|
| **REST API** | 780+ endpoints, JSON | No native REST (XML-RPC) | REST API available |
| **API authentication** | JWT + PAM | SASL, polkit | Ticket-based + PAM |
| **CLI tool** | curl / any HTTP client | virsh | pvesh / pvecm |
| **Language bindings** | Any (HTTP/JSON) | C, Python, Go, Java, Perl | Perl, Python (community) |
| **WebSocket console** | Built-in (binary I/O) | Via VNC/SPICE protocols | noVNC via web UI |
| **Event streaming** | SSE (real-time) | Event loop (libvirt API) | Not available natively |
| **Infrastructure-as-code** | JSON API (Terraform-friendly) | XML templates | Terraform provider available |
| **Pagination** | Built-in (offset/limit) | Not available | Built-in |
| **Rate limiting** | Per-user + global | Not available | Not available |

### API Design Philosophy

Zyvor Fabric exposes every capability through a consistent REST API with JSON payloads. This makes it straightforward to integrate with CI/CD pipelines, custom scripts, and infrastructure-as-code tools without installing language-specific client libraries.

libvirt requires binding to its C library (or using language-specific wrappers) and working with XML documents. While powerful, this creates a higher integration barrier for teams not already familiar with libvirt.

Proxmox provides a REST API, but many advanced features are more easily accessed through the web UI or Perl CLI tools.

---

## VM Lifecycle

| Feature | Zyvor Fabric | libvirt/virsh | Proxmox VE |
|---------|---------|---------------|------------|
| **Create VM** | POST endpoint | `virsh define` + XML | Web UI or `qm create` |
| **Start/stop/restart** | REST endpoints | `virsh start/shutdown/reboot` | Web UI or `qm start/stop` |
| **Pause/resume** | REST endpoints | `virsh suspend/resume` | `qm suspend/resume` |
| **Clone (full)** | REST endpoint (reflink-aware) | `virt-clone` | `qm clone` |
| **Clone (linked)** | qcow2 backing file clone | `virt-clone --reflink` | Linked clone support |
| **Snapshots** | REST endpoint (disk + full) | `virsh snapshot-create` | `qm snapshot` |
| **Snapshot tree** | Tree endpoint with hierarchy | `virsh snapshot-list --tree` | Web UI tree view |
| **Live migration** | Disk-copy path GA; native FluxVM transport preview; full HA cutover pre-GA | `virsh migrate` | Built-in cluster migration |
| **CPU hotplug** | REST endpoint + event | `virsh setvcpus --live` | `qm set --vcpus` |
| **Memory hotplug** | REST endpoint + event | `virsh setmem --live` | `qm set --memory` |
| **Disk hotplug** | REST endpoint + event | `virsh attach-disk` | `qm set --scsi` |
| **Cloud-init** | Built-in ISO generation | Manual or `virt-install` | Built-in snippets |
| **Image building** | mkosi integration | Not built-in | Not built-in |
| **Async operations** | 202 Accepted + SSE events | Synchronous or event loop | Task system with polling |

---

## Networking

| Feature | Zyvor Fabric | libvirt/virsh | Proxmox VE |
|---------|---------|---------------|------------|
| **Network backend** | Direct netlink (rtnetlink), DHCP via a directly-managed dnsmasq process | libvirt virtual networks | Open vSwitch / Linux bridge |
| **Bridge management** | REST API (CRUD) | XML network definitions | Web UI + `/etc/network/interfaces` |
| **VLAN support** | REST API | XML config | Web UI |
| **Bond interfaces** | REST API | Host-level only | Web UI |
| **TAP devices** | REST API | Auto-created | Auto-created |
| **MACVTAP** | REST API | XML config | Limited |
| **Port forwarding** | REST API (nftables) | iptables hook scripts | iptables rules |
| **VXLAN overlays** | REST API | Not built-in | SDN module (subscription) |
| **SR-IOV** | REST API | XML config | PCI passthrough |
| **Network policies** | Built-in policy engine | Not available | SDN firewall (subscription) |
| **Service mesh** | Built-in | Not available | Not available |
| **Traffic shaping** | Built-in | tc scripts | tc scripts |
| **DNS policy** | Built-in DNS manager | dnsmasq integration | Not built-in |
| **VPN mesh** | Built-in (WireGuard) | Not built-in | Not built-in |
| **Packet mirroring** | REST API | Not built-in | Not built-in |
| **NAT gateway** | REST API | Automatic NAT | iptables masquerade |

---

## Storage

| Feature | Zyvor Fabric | libvirt/virsh | Proxmox VE |
|---------|---------|---------------|------------|
| **Local directory** | REST API pool | Storage pool XML | Directory storage |
| **NFS** | REST API pool (NFSv3/v4) | Storage pool | NFS storage |
| **LVM** | REST API pool | Storage pool | LVM storage |
| **LVM thin** | REST API pool | Storage pool | LVM-thin storage |
| **ZFS** | REST API pool | Community support | ZFS storage (built-in) |
| **Ceph RBD** | REST API pool | Storage pool | Ceph storage (built-in) |
| **iSCSI** | Not built-in | Storage pool | iSCSI storage |
| **GlusterFS** | Not built-in | Storage pool | Not built-in |
| **Distributed storage** | Basic replication API | Not built-in | Ceph integration |
| **Image formats** | qcow2, raw | qcow2, raw, vmdk, vdi, etc. | qcow2, raw, vmdk |

---

## Security

| Feature | Zyvor Fabric | libvirt/virsh | Proxmox VE |
|---------|---------|---------------|------------|
| **Authentication** | PAM + JWT | SASL + polkit | PAM + PVE + LDAP/AD |
| **Authorization** | RBAC (Admin/User/Viewer) | polkit policies | RBAC with path-based ACLs |
| **Audit logging** | Built-in (structured) | libvirt audit hooks | Syslog-based |
| **Rate limiting** | Per-user + global login | Not available | Not available |
| **Input validation** | Server-side + sanitization | XML schema validation | Parameter validation |
| **VM isolation** | cgroup + namespace (systemd) | SELinux/AppArmor sVirt | AppArmor |
| **Network firewall** | Per-VM firewall API | nwfilter XML rules | pve-firewall |
| **Encryption** | VM encryption API | Disk encryption (LUKS) | Disk encryption |
| **Token revocation** | Built-in (JTI-based) | Not applicable | Not available |
| **Shell injection prevention** | Metacharacter blocking | Not applicable (no shell API) | Not applicable |

---

## Monitoring and Observability

| Feature | Zyvor Fabric | libvirt/virsh | Proxmox VE |
|---------|---------|---------------|------------|
| **Health endpoint** | `/health` + `/readyz` (HTTP) | Not available | `/api2/json/version` |
| **VM metrics** | REST API (CPU, memory, I/O) | `virsh domstats` | RRD graphs via web UI |
| **Real-time events** | SSE stream (16 event types) | Event loop API | Not available natively |
| **Notification channels** | Email, Slack, Webhook, Teams | Not built-in | Email only |
| **Alert rules** | Configurable per event/severity | Not built-in | Email notifications |
| **Webhook retry** | Exponential backoff (10 retries) | Not applicable | Not applicable |
| **Host resource stats** | REST API | `virsh nodeinfo` | Web UI + API |
| **NUMA topology** | REST API + placement advisor | `virsh capabilities` | Not exposed |
| **Log integration** | journald (native) | libvirtd logs | Syslog + tasklog |

---

## Operations

| Feature | Zyvor Fabric | libvirt/virsh | Proxmox VE |
|---------|---------|---------------|------------|
| **Backup types** | Full + incremental | Not built-in (use external) | Full + incremental (vzdump) |
| **Backup scheduling** | Policy-based (daily/weekly/monthly) | Not built-in | Scheduled vzdump |
| **Backup retention** | Per-backup retention days | Not applicable | Retention count |
| **Restore** | REST API (in-place + new VM) | Not built-in | vzdump restore |
| **Templates** | Template API | Template volumes | VM templates |
| **Resource quotas** | Per-user/group quotas | Not available | Pool-based limits |
| **Autoscaling** | Built-in autoscale API | Not available | Not available |
| **Multi-tenancy** | Tenant isolation API | Not available | Pool-based separation |
| **Web UI** | React-based (hypersdk design) | virt-manager (desktop app) | Built-in web UI |
| **Configuration management** | Declarative API | XML definitions | pvecm + web UI |

---

## Summary

### Choose Zyvor Fabric when:

- You want **VM lifecycle with no systemd dependency** -- FluxVM supervises each VM's process directly, with resource control in standard cgroup hierarchies and no systemd unit involved.
- You need a **comprehensive REST API** (780+ endpoints) for automation-first infrastructure management.
- You want **built-in networking** (direct netlink, no systemd-networkd dependency) including bridges, VLANs, bonds, VXLAN overlays, SR-IOV, network policies, and traffic shaping -- all manageable via API.
- You prefer a **modern technology stack** (Rust backend, React frontend) with strong type safety and memory safety.
- You need **real-time observability** via SSE events and multi-channel notifications (Email, Slack, Webhook, Teams) with webhook retry.

### Choose libvirt when:

- You have **existing tooling** built on libvirt XML domain definitions and virsh.
- You need support for **multiple hypervisors** (QEMU/KVM, Xen, LXC) behind a unified API.
- You need support for a **wide range of disk formats** (vmdk, vdi, vpc, etc.).
- You want the **most mature and widely-deployed** Linux virtualization management layer.

### Choose Proxmox VE when:

- You need a **turnkey multi-node cluster** with built-in shared storage (Ceph), live migration, and HA fencing.
- You want a **feature-rich web UI** for day-to-day management without API knowledge.
- You need **both VMs and containers** (LXC) managed from a single interface.
- You have a **commercial support** requirement and are willing to purchase a subscription.
