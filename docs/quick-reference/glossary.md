# Glossary

Definitions of terms used throughout the Zyvor Fabric documentation and codebase.

---

## A

**Affinity Rule**: A DRS constraint that specifies whether certain VMs should run
on the same host (affinity) or on different hosts (anti-affinity).

**Anti-Affinity**: A scheduling constraint that ensures two or more VMs are placed
on different physical hosts, improving fault tolerance.

**AppState**: The central shared state structure in Zyvor Fabric, wrapped in `Arc` and
injected into every Axum handler. Contains the state store, config, drivers, and
all subsystem managers.

**Audit Log**: A structured record of every state-changing operation performed
through the Zyvor Fabric API, including the user, action, resource, and outcome.

**Autoscale**: Automatic adjustment of the number of VM instances based on
resource utilization metrics or custom triggers.

**Axum**: The async Rust web framework used by Zyvor Fabric. Built on top of
`tower` and `hyper`, running on the Tokio async runtime.

## B

**Balloon Driver**: A mechanism for dynamically adjusting VM memory allocation.
The guest kernel's balloon driver inflates (reclaims memory from guest) or
deflates (returns memory to guest) on demand.

**Baseline**: In lifecycle management, a set of expected software versions or
configurations that a host should match. Used for compliance scanning.

**Bearer Token**: An HTTP authentication scheme where the client presents a JWT
token in the `Authorization: Bearer <token>` header.

**Bridge**: A virtual network switch that connects multiple network interfaces.
VMs connect to the host network through a bridge device.

## C

**CancellationToken**: A Tokio utility (`tokio_util::sync::CancellationToken`)
used to signal graceful shutdown to background tasks.

**Cgroup (Control Group)**: A Linux kernel feature for organizing processes into
hierarchical groups with resource limits (CPU, memory, I/O).

**Claims**: The payload of a JWT token containing the subject (username), role,
expiration, and token ID.

**Cloud-Init**: An industry-standard tool for early initialization of cloud
instances. Zyvor Fabric generates cloud-init NoCloud datasource ISOs for VM
provisioning.

**Clone**: Creating a copy of an existing VM, including its disk image and
metadata, under a new name.

**Cluster**: A group of physical hosts managed as a unit, enabling features like
DRS, HA, and live migration.

**Content Library**: A centralized repository for VM images, templates,
customization specs, and host profiles that can be shared across sites.

**CORS (Cross-Origin Resource Sharing)**: HTTP headers that control which web
origins can make API requests. Configured via `daemon.cors_origins`.

## D

**Datacenter**: The top level of the Zyvor Fabric organizational hierarchy, containing
one or more clusters.

**DHCP Server**: Dynamic Host Configuration Protocol server for automatic VM IP
address assignment, run as a directly-managed `dnsmasq` process (not
systemd-networkd's built-in DHCP server).

**Disk Image**: A file containing the contents of a virtual disk. Common formats
include raw (`.raw`), QCOW2 (`.qcow2`), VMDK, and VDI.

**Distributed Resource Scheduler (DRS)**: An automated system that balances VM
workloads across cluster hosts by recommending or executing migrations.

**Distributed Storage (SDRS)**: Storage DRS. Balances VM storage across datastore
clusters based on capacity and I/O load.

**DNS Policy**: Per-VM DNS configuration including zone assignments, custom
records, and resolution policies.

**Driver**: An abstraction layer between Zyvor Fabric and the underlying VM
lifecycle tooling. The `VmDriver` trait family defines lifecycle, resource
control, logs, images, shell, and console operations. Implemented against
FluxVM (see "FluxVM") -- there's no other backend to choose.

## E

**FluxVM**: A disposable-VM engine ([github.com/zyvorai/fluxvm](https://github.com/zyvorai/fluxvm))
with no systemd dependency, reached over its REST API and vsock guest agent
(`driver.fluxvm_url` in `zyvor-fabricd.toml`). Zyvor Fabric's only VM
driver.

**Encryption**: VM disk encryption using key management providers. Supports
per-VM encryption policies and key rotation.

**Entity**: A generic term for any persistable object in the state store (VM,
snapshot, template, policy, etc.).

**Event (SSE)**: A Server-Sent Event broadcast to connected clients when a VM
state change or significant system event occurs.

## F

**Failover**: Automatic transfer of VM execution from a failed host to a healthy
host in an HA cluster.

**Fault Tolerance (FT)**: Continuous VM replication that enables instant failover
with zero downtime and zero data loss.

**Firewall Profile**: A named set of firewall rules (allow/deny) that can be
assigned to one or more VMs.

**Floating IP**: A portable IP address that can be moved between VMs for high
availability or migration purposes.

## G

**Guest**: The operating system and applications running inside a virtual machine.

**Guest Agent**: Software running inside a VM that communicates with the
hypervisor for operations like graceful shutdown and filesystem freeze.

## H

**HA (High Availability)**: A configuration that automatically restarts VMs on
surviving hosts when a host fails.

**Heartbeat**: A periodic signal sent by cluster hosts to the controller to
indicate they are alive and healthy.

**Host**: A physical machine running the hypervisor, capable of hosting VMs.

**Host Agent**: The `host-agent` component that runs on each cluster host, reports
resource availability, and executes controller commands.

**Host Profile**: A configuration template that defines expected host settings for
compliance checking.

**Hotplug**: Adding or removing hardware (CPU, memory, disk, NIC) from a running
VM without requiring a reboot.

**Hugepages**: Large memory pages (2MB or 1GB) that reduce TLB misses and improve
VM memory access performance.

## I

**Image**: A disk image file used as the root filesystem for a VM.

**Instance Type**: See "Profile".

## J

**journald**: The systemd journal daemon that captures and stores log messages.
Zyvor Fabric logs are available via `journalctl -u Zyvor Fabric`.

**JSON Web Token (JWT)**: A compact, URL-safe token format used for
authentication. Contains encoded claims (user, role, expiration).

**JTI (JWT ID)**: A unique identifier embedded in each JWT token, used for
token revocation.

## K

**KSM (Kernel Same-page Merging)**: A Linux kernel feature that deduplicates
identical memory pages across VMs, reducing total memory usage.

**KVM (Kernel-based Virtual Machine)**: A Linux kernel module that enables
hardware-assisted virtualization using Intel VT-x or AMD-V extensions.

## L

**Labels**: Key-value pairs attached to a VM for categorization and querying.

**LDAP**: Lightweight Directory Access Protocol. Used for external user
authentication against a directory service.

**Lifecycle Manager**: Manages host software lifecycle: baseline definitions,
compliance scanning, remediation, and rolling updates.

**Live Migration**: Moving a VM between hosts. Fabric’s production disk-copy path
uses iterative rsync (brief cutover pause). Native FluxVM VMM transport +
receivers is preview until KVM shared-disk e2e gates pass.

**Lock Manager**: Provides per-resource advisory locks with TTL to prevent
concurrent conflicting operations.

**LVM (Logical Volume Manager)**: A Linux storage management system that provides
flexible disk partitioning. Zyvor Fabric supports LVM and LVM-thin storage pools.

## M

**machinectl / Machined (systemd-machined)**: **Removed** from Fabric. VM
lifecycle is FluxVM only — use Virtual Machines (`/app/vms`) and `/api/vms`.

**macvtap**: A Linux network device that combines the functionality of macvlan
and TAP devices for efficient VM networking.

**Maintenance Mode**: A host state where no new VMs are placed and existing VMs
can be migrated away for host servicing.

**Metrics**: Quantitative measurements of system and VM performance (CPU usage,
memory usage, disk I/O, network throughput).

**Mirror Session**: A packet mirroring configuration that copies network traffic
from a source to a destination for analysis.

## N

**NAT (Network Address Translation)**: Remapping IP addresses in network packets.
Zyvor Fabric supports SNAT, DNAT, and NAT pools.

**Nested Virtualization**: Running a hypervisor inside a VM, enabling VMs to host
their own VMs.

**netlink**: A Linux kernel interface for communication between the kernel and
userspace processes, used for network configuration.

**networkctl**: A systemd command-line tool for managing network links and viewing
network status.

**nftables**: The Linux kernel packet filtering framework that replaced iptables.
Used by Zyvor Fabric for firewall rules and NAT.

**NoCloud**: A cloud-init datasource that reads configuration from a local
filesystem or attached ISO, without requiring a metadata service.

**NUMA (Non-Uniform Memory Access)**: A memory architecture where memory access
time depends on the memory location relative to a processor. NUMA-aware placement
improves VM performance.

**NVRAM**: Non-volatile RAM used to store UEFI firmware settings.

## O

**OIDC (OpenID Connect)**: An identity layer on top of OAuth 2.0, used for
external authentication with identity providers.

**OVA (Open Virtual Appliance)**: A single-file packaging format for virtual
machines, containing OVF metadata and one or more disk images.

**Overlay Network**: A virtual network built on top of an existing network,
typically using tunneling protocols like VXLAN or WireGuard.

**OpenStack Compatibility**: Experimental façade on `zyvor-fabricd` that speaks
Keystone / Nova / Glance / Neutron / Cinder wire protocols at `/identity`,
`/compute`, `/image`, `/network`, and `/volume`. Does not replace FluxVM; see
`docs/openstack-compat.md`.

## P

**Pagination**: Splitting large result sets into pages. Zyvor Fabric uses
`offset`/`limit` query parameters with a maximum limit of 1000.

**PAM (Pluggable Authentication Module)**: A Linux framework for authentication.
Zyvor Fabric uses PAM to authenticate users against the system.

**Pause**: Suspending VM execution (freezing all vCPUs) without deallocating
resources. The VM can be resumed instantly.

**Per-VM Lock**: A mutex acquired before performing state-changing operations on
a specific VM, preventing race conditions.

**Plugin**: An extension module registered with the Zyvor Fabric plugin registry for
custom functionality.

**Policy Engine**: The `network-policy` component that evaluates network access
control rules for VM traffic.

**Predictive DRS**: An advanced DRS mode that uses historical data and trend
analysis to proactively place VMs before resource contention occurs.

**Profile**: A predefined set of VM resource specifications (CPU, memory, disk)
similar to cloud instance types.

**Prometheus**: An open-source monitoring system. Zyvor Fabric exposes metrics in
Prometheus format at the `/metrics` endpoint.

**Public URL**: External base URL (`daemon.public_url` /
`ZYVOR_FABRICD_PUBLIC_URL`) advertised in the OpenStack service catalog and
similar client-facing links. When unset, derived from `daemon.listen` and TLS.

## Q

**QCOW2**: QEMU Copy-On-Write version 2. A disk image format supporting thin
provisioning, snapshots, compression, and encryption.

**QEMU**: Quick Emulator. A generic machine emulator and virtualizer used by
Zyvor Fabric (via KVM) as the hypervisor.

**QoS (Quality of Service)**: Traffic management policies that prioritize,
rate-limit, or guarantee bandwidth for VM network traffic.

**Quota**: A resource limit applied to a user or group, restricting the total
number of VMs, CPUs, memory, or storage they can consume.

## R

**RBAC (Role-Based Access Control)**: An authorization model where permissions are
assigned to roles (Admin, User, Viewer) rather than individual users.

**Reconciler**: A background task that periodically compares desired state with
actual state and corrects any drift.

**Recovery Point Objective (RPO)**: The maximum acceptable amount of data loss
measured in time. Zyvor Fabric monitors RPO violations for replicated VMs.

**Remediation**: The process of bringing a non-compliant host into compliance with
a defined baseline by applying updates or configuration changes.

**Replication**: Continuously copying VM data from a primary site to a secondary
site for disaster recovery.

**Resource Pool**: A logical partition of cluster resources (CPU, memory) assigned
to a group of VMs, with admission control to enforce limits.

**Reverse Proxy**: A server (nginx, Caddy) that sits in front of Zyvor Fabric,
handling TLS termination and request routing.

**Rolling Update**: Updating hosts in a cluster one at a time, ensuring continuous
service availability during the update process.

## S

**Scheduler**: A background task that executes VM operations (start, stop, backup)
at specified times based on cron-like schedules.

**Secure Boot**: A UEFI firmware feature that verifies the digital signature of
boot loaders before execution, preventing unauthorized code from running.

**Service Mesh**: A network infrastructure layer that provides service discovery,
load balancing, health checking, and traffic management for VM workloads.

**SCIM (System for Cross-domain Identity Management)**: Protocol used by Entra ID /
Okta to provision Fabric users and groups at `/scim/v2`. Separate from the
OpenStack Keystone-compat `/identity` surface. See `docs/scim-identity.md`.

**Site Recovery**: Disaster recovery orchestration including recovery plans,
planned migration, disaster failover, and reprotection workflows.

**SMBIOS**: System Management BIOS. Firmware tables that describe hardware
configuration. Zyvor Fabric can inject custom SMBIOS data into VMs.

**Snapshot**: A point-in-time capture of a VM's state (disk and optionally memory)
that can be reverted to later.

**Socket Activation**: A systemd feature where a service is started on-demand when
a connection arrives on its listening socket.

**spawn_bg!**: A Rust macro defined in Zyvor Fabric that spawns a cancellable
background task with shared state access.

**Spot Instance**: A VM that can be preempted (evicted) when resources are needed,
typically offered at a lower priority than regular VMs.

**SR-IOV (Single Root I/O Virtualization)**: A hardware standard that allows a
single physical network adapter to present multiple virtual functions to VMs for
near-native network performance.

**SSE (Server-Sent Events)**: A web technology for pushing real-time updates from
server to client over HTTP. Zyvor Fabric uses SSE for VM state change notifications.

**State Store**: The `state-store` crate providing persistent JSON file storage
with an in-memory cache.

**systemd-machined**: Removed from Fabric — see "machinectl / Machined". Use FluxVM / Virtual Machines.

**systemd-networkd**: A systemd service that manages network configuration on
Linux. Zyvor Fabric does not depend on it -- host networking (bridges, VLANs,
bonds, VXLAN, etc.) is applied directly via netlink calls.

**systemd-vmspawn**: Historical systemd VM spawn tool. Not used by Fabric; FluxVM owns VM lifecycle.

## T

**Tags**: String labels attached to VMs for organization and filtering.

**TAP Device**: A virtual network interface that operates at the data link layer.
VMs connect to host networking via TAP devices bridged to a host bridge.

**tc (Traffic Control)**: A Linux kernel subsystem for network traffic management,
used by Zyvor Fabric for QoS and traffic shaping.

**Template**: A VM configuration blueprint that can be used to quickly deploy new
VMs with predefined settings.

**Tokio**: The async runtime used by Zyvor Fabric. Provides task scheduling, async
I/O, timers, and synchronization primitives.

**tower**: A Rust library of modular and reusable components for building robust
networking clients and servers. Axum is built on tower.

**TPM (Trusted Platform Module)**: A security chip (or virtual equivalent) that
provides cryptographic functions for secure boot, key storage, and attestation.

## U

**UEFI (Unified Extensible Firmware Interface)**: A modern firmware interface
replacing legacy BIOS. Supports Secure Boot and larger disk support via GPT.

**User Database**: The SQLite database (`auth.db`) that stores Zyvor Fabric user
accounts, password hashes, and roles.

## V

**vCPU**: A virtual CPU allocated to a VM, mapped to physical CPU threads via
the hypervisor.

**VLAN (Virtual LAN)**: A logical network partition at the data link layer
(IEEE 802.1Q). Zyvor Fabric can create VLAN devices directly via netlink.

**VM (Virtual Machine)**: An isolated computing environment with its own virtual
hardware (CPU, memory, disk, network), running its own operating system.

**fabricctl**: The Zyvor Fabric command-line client for managing VMs via the REST API.


**VMDriver**: The trait defined in `Zyvor Fabric-driver-core` that abstracts VM
lifecycle operations (start, stop, reboot, state query).

**Zyvor Fabric**: The main daemon process that hosts the REST API, WebSocket server,
and background task system.

**VMState**: An enumeration of possible VM states: Running, Stopped, Paused,
Starting, Stopping, Failed, Unknown.

**VNC (Virtual Network Computing)**: A graphical desktop sharing system. Zyvor Fabric
provides a VNC proxy for browser-based graphical VM console access.

**VPN Mesh**: A network of WireGuard VPN tunnels connecting VMs across hosts or
sites, forming an overlay network.

**vSock**: A socket type for efficient communication between a VM guest and the
host without requiring network configuration.

**VXLAN (Virtual Extensible LAN)**: A network overlay technology that encapsulates
Layer 2 Ethernet frames within Layer 4 UDP packets, enabling multi-host VM
networking.

## W

**WebSocket**: A full-duplex communication protocol over a single TCP connection.
Zyvor Fabric uses WebSockets for real-time VM console access.

**Webhook**: An HTTP callback triggered by Zyvor Fabric when specific events occur,
allowing integration with external systems.

**WireGuard**: A modern, high-performance VPN protocol used by Zyvor Fabric for VPN
mesh networking between VMs.

## Z

**zbus**: A Rust crate for D-Bus communication. No longer a Zyvor Fabric
dependency -- it was used by the `machinectl`/`machined-dbus` VM driver,
which is deleted (see "Driver").

**Zone (Availability)**: A logical or physical grouping of hosts within a
datacenter, used for fault isolation and placement decisions.

**Zone (Firewall)**: A named grouping of firewall rules applied to network
interfaces, similar to firewalld zones.

**ZFS**: A combined file system and volume manager originally designed by Sun
Microsystems. Zyvor Fabric supports ZFS as a storage pool backend.


**/app**: Authenticated Zyvor Fabric console (VMs, network, storage, ops).

**/sign-in**: Sign-in page for the console (legacy `/sign-in` redirects here).
