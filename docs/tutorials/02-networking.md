# Tutorial 02: VM Networking

Configure virtual networking for your VMs. This tutorial covers bridges,
VLANs, bond interfaces, port forwarding, network policies, and DNS
configuration.

> **Two policy planes:** Fabric **Net Security** network policies are host SDN
> (label → nftables). Per-VM TC/eBPF edge policy is the **Dataplane** tab /
> `/api/vms/{name}/dataplane/*` and cluster `/api/dataplane/*` — see
> [VM edge dataplane](../guides/vm-drivers/fluxvm-dataplane.md) and
> [Tutorial 09](09-edge-dataplane.md). This tutorial focuses on bridges, VLANs,
> forwards, and Fabric SDN.

**Level:** Intermediate
**Time:** 45 minutes
**Prerequisites:** Completed [Tutorial 01](01-first-vm.md), Zyvor Fabric running

---

## What You Will Learn

1. Create bridge networks for VM connectivity
2. Set up VLAN segmentation
3. Configure bond interfaces for redundancy
4. Define port forwarding rules
5. Apply network policies (Kubernetes-style)
6. Manage DNS zones and policies

---

## Setup

```bash
export FABRIC_HOST="http://localhost:3000"
TOKEN=$(curl -s "$FABRIC_HOST/api/auth/login" \
  -H "Content-Type: application/json" \
  -d '{"username": "admin", "password": "your-password"}' | jq -r '.token')
```

---

## Network Architecture Overview

Zyvor Fabric manages networking through direct netlink calls (rtnetlink) —
every network change (bridges, VLANs, bonds, TAP/macvtap devices, addresses)
takes effect immediately in the kernel, with no config files written and no
`networkctl reload` step. This works identically whether or not systemd is
present on the host. WireGuard mesh interfaces follow the same pattern; the
DHCP server described in Step 7 below is the one piece that still shells out
to an external tool (`dnsmasq`, not systemd-networkd).

```
                         +------------------+
                         |   Physical NIC   |
                         |    (enp0s3)       |
                         +--------+---------+
                                  |
                    +-------------+-------------+
                    |                           |
              +-----+-----+             +------+------+
              | vm-bridge  |             | vlan100     |
              | (bridge)   |             | (VLAN)      |
              +-----+-----+             +------+------+
                    |                           |
              +-----+-----+             +------+------+
              | vm-web-01  |             | vm-db-01   |
              | (TAP)      |             | (TAP)      |
              +------------+             +-------------+
```

---

## Step 1: Bridge Networks

A bridge connects multiple VM TAP interfaces into a shared Layer 2 domain, much
like a virtual switch.

### Create a Bridge

```bash
curl -s -X POST "$FABRIC_HOST/api/networkd/bridges" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "vm-bridge",
    "stp": true,
    "forward_delay_sec": 4,
    "hello_time_sec": 2,
    "max_age_sec": 20,
    "vlan_filtering": false,
    "mtu": 1500,
    "addresses": ["192.168.100.1/24"],
    "gateway": null,
    "dns": ["8.8.8.8", "1.1.1.1"],
    "dhcp": false
  }' | jq .
```

Expected response:

```json
{
  "id": "c3d4e5f6-a7b8-9012-cdef-345678901234",
  "name": "vm-bridge",
  "stp": true,
  "forward_delay_sec": 4,
  "hello_time_sec": 2,
  "max_age_sec": 20,
  "vlan_filtering": false,
  "mtu": 1500,
  "addresses": ["192.168.100.1/24"],
  "gateway": null,
  "dns": ["8.8.8.8", "1.1.1.1"],
  "dhcp": false,
  "created": "2026-04-12T11:00:00Z",
  "updated": "2026-04-12T11:00:00Z"
}
```

### Bridge Parameters Reference

| Parameter          | Type     | Description                                |
|-------------------|----------|--------------------------------------------|
| `name`            | string   | Network interface name                     |
| `stp`             | bool     | Enable Spanning Tree Protocol              |
| `forward_delay_sec`| integer | STP forward delay in seconds               |
| `hello_time_sec`  | integer  | STP hello time in seconds                  |
| `max_age_sec`     | integer  | STP max age in seconds                     |
| `vlan_filtering`  | bool     | Enable per-VLAN filtering on the bridge    |
| `mtu`             | integer  | Maximum transmission unit                  |
| `mac_address`     | string   | Override MAC address (optional)            |
| `addresses`       | string[] | Static IP addresses in CIDR notation       |
| `gateway`         | string   | Default gateway IP (optional)              |
| `dns`             | string[] | DNS server addresses                       |
| `dhcp`            | bool     | Enable DHCP client on the bridge           |

### List Bridges

```bash
curl -s "$FABRIC_HOST/api/networkd/bridges" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

### Get a Specific Bridge

```bash
curl -s "$FABRIC_HOST/api/networkd/bridges/c3d4e5f6-a7b8-9012-cdef-345678901234" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

### Update a Bridge

```bash
curl -s -X PUT "$FABRIC_HOST/api/networkd/bridges/c3d4e5f6-a7b8-9012-cdef-345678901234" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "vm-bridge",
    "stp": true,
    "forward_delay_sec": 2,
    "hello_time_sec": 1,
    "max_age_sec": 10,
    "vlan_filtering": true,
    "mtu": 9000,
    "addresses": ["192.168.100.1/24"],
    "dns": ["8.8.8.8"],
    "dhcp": false
  }' | jq .
```

### Delete a Bridge

```bash
curl -s -X DELETE "$FABRIC_HOST/api/networkd/bridges/c3d4e5f6-a7b8-9012-cdef-345678901234" \
  -H "Authorization: Bearer $TOKEN"

# Returns 204 No Content
```

### Using a Bridge with a VM

When starting a VM, enable TAP networking to attach it to a bridge:

```bash
curl -s -X POST "$FABRIC_HOST/api/vms/web-server/start" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "network_tap": true,
    "kvm": true
  }' | jq .
```

---

## Step 2: VLANs

VLANs provide Layer 2 isolation on top of a physical or bridge interface.

### Create a VLAN

```bash
curl -s -X POST "$FABRIC_HOST/api/networkd/vlans" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "vlan-database",
    "vlan_id": 100,
    "parent_interface": "vm-bridge",
    "mtu": 1500,
    "addresses": ["10.100.0.1/24"],
    "gateway": null,
    "dns": ["10.100.0.1"],
    "dhcp": false
  }' | jq .
```

Expected response:

```json
{
  "id": "d4e5f6a7-b8c9-0123-defg-456789012345",
  "name": "vlan-database",
  "vlan_id": 100,
  "parent_interface": "vm-bridge",
  "mtu": 1500,
  "addresses": ["10.100.0.1/24"],
  "gateway": null,
  "dns": ["10.100.0.1"],
  "dhcp": false,
  "created": "2026-04-12T11:05:00Z",
  "updated": "2026-04-12T11:05:00Z"
}
```

### Create a Second VLAN for Application Servers

```bash
curl -s -X POST "$FABRIC_HOST/api/networkd/vlans" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "vlan-app",
    "vlan_id": 200,
    "parent_interface": "vm-bridge",
    "mtu": 1500,
    "addresses": ["10.200.0.1/24"],
    "dns": ["10.200.0.1"],
    "dhcp": false
  }' | jq .
```

### List VLANs

```bash
curl -s "$FABRIC_HOST/api/networkd/vlans" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

### VLAN Topology Example

```
                   +------------------+
                   |    vm-bridge     |
                   |  192.168.100.1   |
                   +--------+---------+
                            |
              +-------------+-------------+
              |                           |
        +-----+------+            +------+------+
        | VLAN 100   |            | VLAN 200    |
        | 10.100.0/24|            | 10.200.0/24 |
        | (database) |            | (app)       |
        +-----+------+            +------+------+
              |                           |
        +-----+------+            +------+------+
        | db-01      |            | app-01      |
        | db-02      |            | app-02      |
        +------------+            +-------------+
```

---

## Step 3: Bond Interfaces

Bonds aggregate multiple network interfaces for redundancy or increased
throughput.

### Create a Bond

```bash
curl -s -X POST "$FABRIC_HOST/api/networkd/bonds" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "bond0",
    "mode": "802.3ad",
    "members": ["enp0s8", "enp0s9"],
    "mtu": 9000,
    "lacp_rate": "fast",
    "xmit_hash_policy": "layer3+4",
    "addresses": ["10.0.0.1/24"],
    "gateway": "10.0.0.254",
    "dns": ["10.0.0.1"],
    "dhcp": false
  }' | jq .
```

Expected response:

```json
{
  "id": "e5f6a7b8-c9d0-1234-efgh-567890123456",
  "name": "bond0",
  "mode": "802.3ad",
  "members": ["enp0s8", "enp0s9"],
  "mtu": 9000,
  "lacp_rate": "fast",
  "xmit_hash_policy": "layer3+4",
  "addresses": ["10.0.0.1/24"],
  "gateway": "10.0.0.254",
  "dns": ["10.0.0.1"],
  "dhcp": false,
  "created": "2026-04-12T11:10:00Z",
  "updated": "2026-04-12T11:10:00Z"
}
```

### Bond Modes

| Mode           | Description                                    |
|---------------|------------------------------------------------|
| `balance-rr`  | Round-robin load balancing                     |
| `active-backup`| Only one member active; failover on link loss |
| `balance-xor` | Hash-based distribution                        |
| `broadcast`   | All members transmit every frame               |
| `802.3ad`     | IEEE 802.3ad LACP (requires switch support)    |
| `balance-tlb` | Adaptive transmit load balancing               |
| `balance-alb` | Adaptive load balancing (RX + TX)              |

### List Bonds

```bash
curl -s "$FABRIC_HOST/api/networkd/bonds" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

---

## Step 4: Port Forwarding

Forward host ports to VM services. This is implemented through nftables rules.

### Create a Port Forward

Forward host port 8080 to a VM's port 80:

```bash
curl -s -X POST "$FABRIC_HOST/api/networkd/port-forwards" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "web-forward",
    "protocol": "tcp",
    "host_port": 8080,
    "vm_ip": "192.168.100.10",
    "vm_port": 80,
    "description": "Forward HTTP traffic to web VM"
  }' | jq .
```

Expected response:

```json
{
  "id": "f6a7b8c9-d0e1-2345-fghi-678901234567",
  "name": "web-forward",
  "protocol": "tcp",
  "host_port": 8080,
  "vm_ip": "192.168.100.10",
  "vm_port": 80,
  "description": "Forward HTTP traffic to web VM",
  "enabled": true,
  "created": "2026-04-12T11:15:00Z"
}
```

### Forward SSH Access

```bash
curl -s -X POST "$FABRIC_HOST/api/networkd/port-forwards" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "ssh-web-vm",
    "protocol": "tcp",
    "host_port": 2222,
    "vm_ip": "192.168.100.10",
    "vm_port": 22,
    "description": "SSH access to web VM"
  }' | jq .
```

Now you can SSH to the VM through the host:

```bash
ssh -p 2222 user@host-ip
```

### List Port Forwards

```bash
curl -s "$FABRIC_HOST/api/networkd/port-forwards" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

---

## Step 5: Network Policies

Network policies provide Kubernetes-style ingress/egress rules at the VM level.
They use label selectors to target VMs and define allowed traffic flows.

### Create a Network Policy

Allow the `app` tier to receive HTTP traffic and communicate with the `database`
tier:

```bash
curl -s -X POST "$FABRIC_HOST/api/network-policies" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "allow-app-traffic",
    "description": "Allow HTTP inbound and DB egress for app tier",
    "endpoint_selector": {
      "match_labels": {
        "tier": "app"
      }
    },
    "ingress": [
      {
        "ports": [{"protocol": "tcp", "port": 80}, {"protocol": "tcp", "port": 443}],
        "from": [
          {"cidr": "0.0.0.0/0"}
        ]
      }
    ],
    "egress": [
      {
        "ports": [{"protocol": "tcp", "port": 5432}],
        "to": [
          {"match_labels": {"tier": "database"}}
        ]
      },
      {
        "ports": [{"protocol": "tcp", "port": 53}, {"protocol": "udp", "port": 53}],
        "to": [
          {"cidr": "0.0.0.0/0"}
        ]
      }
    ]
  }' | jq .
```

Expected response:

```json
{
  "id": "a7b8c9d0-e1f2-3456-ghij-789012345678",
  "name": "allow-app-traffic",
  "description": "Allow HTTP inbound and DB egress for app tier",
  "endpoint_selector": {
    "match_labels": {"tier": "app"}
  },
  "ingress": [...],
  "egress": [...],
  "status": "active",
  "created": "2026-04-12T11:20:00Z",
  "updated": "2026-04-12T11:20:00Z"
}
```

### Deny All Traffic by Default

Create a default-deny policy, then allowlist specific flows:

```bash
curl -s -X POST "$FABRIC_HOST/api/network-policies" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "default-deny-all",
    "description": "Deny all traffic by default for production VMs",
    "endpoint_selector": {
      "match_labels": {
        "env": "production"
      }
    },
    "ingress": [],
    "egress": []
  }' | jq .
```

### List Network Policies

```bash
curl -s "$FABRIC_HOST/api/network-policies" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

---

## Step 6: DNS Configuration

Zyvor Fabric supports internal DNS zones for VM name resolution.

### Create a DNS Zone

```bash
curl -s -X POST "$FABRIC_HOST/api/dns/zones" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "vm.internal",
    "description": "Internal DNS zone for all VMs"
  }' | jq .
```

Expected response:

```json
{
  "id": "b8c9d0e1-f2a3-4567-hijk-890123456789",
  "name": "vm.internal",
  "description": "Internal DNS zone for all VMs",
  "created": "2026-04-12T11:25:00Z",
  "updated": "2026-04-12T11:25:00Z"
}
```

### Create a DNS Policy

DNS policies control which VMs can resolve which domains:

```bash
curl -s -X POST "$FABRIC_HOST/api/dns/policies" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "app-dns-policy",
    "description": "DNS policy for application tier",
    "endpoint_selector": {
      "match_labels": {"tier": "app"}
    },
    "allowed_zones": ["vm.internal"],
    "upstream_servers": ["8.8.8.8", "1.1.1.1"],
    "block_external": false
  }' | jq .
```

### List DNS Zones

```bash
curl -s "$FABRIC_HOST/api/dns/zones" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

---

## Step 7: DHCP Server Configuration

Zyvor Fabric can configure a DHCP server on a bridge interface. Under the hood
this runs a directly-managed `dnsmasq` process per bridge, not systemd-
networkd's built-in `[DHCPServer]` — it works the same way whether or not
systemd is present on the host. VMs attached to the bridge will receive IP
addresses automatically.

### Configure DHCP on a Bridge

```bash
curl -s -X POST "$FABRIC_HOST/api/networkd/dhcp" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "bridge_name": "vm-bridge",
    "pool_start": "192.168.100.100",
    "pool_end": "192.168.100.199",
    "dns_servers": ["192.168.100.1"],
    "lease_time_sec": 3600
  }' | jq .
```

Expected response:

```json
{
  "status": "configured",
  "bridge": "vm-bridge",
  "pool_offset": 100,
  "pool_size": 100
}
```

### DHCP Server Parameters

| Parameter        | Type            | Description                                              |
|-------------------|-----------------|------------------------------------------------------------|
| `bridge_name`      | string          | Bridge interface to serve DHCP on                          |
| `pool_start`       | string          | First address in the DHCP pool (e.g., `192.168.100.100`)   |
| `pool_end`         | string          | Last address in the DHCP pool — must be in the same /24 as `pool_start` |
| `dns_servers`      | array\<string\> | Optional. DNS server addresses to advertise to clients (default: `1.1.1.1`, `8.8.8.8`) |
| `lease_time_sec`   | integer         | Optional. Lease duration in seconds (default: `3600`)      |

### How It Works

1. `pool_start`/`pool_end` are converted into a pool offset/size relative to
   their shared `/24`; the bridge's own gateway is assumed to be that
   subnet's `.1` (e.g. `192.168.100.1` for the example above).
2. Zyvor Fabric writes a small `dnsmasq` config to
   `/run/zyvor-fabricd/dnsmasq/<bridge>.conf` and starts (or restarts) a
   `dnsmasq` process scoped to that one bridge interface, with its own DNS
   listener disabled (`port=0` — it only serves DHCP leases plus a DNS-server
   option pushed to clients, not a resolver of its own).
3. VMs on the bridge receive IPs from the configured pool immediately — no
   reload step, no systemd unit involved.

### Setting the Pool Range

Pass the actual first and last addresses you want handed out — `pool_start`
and `pool_end` must fall in the same `/24`. For example, `pool_start:
"192.168.100.100"`, `pool_end: "192.168.100.199"` serves 100 addresses
starting at `.100`.

### DNS Server Configuration

`dns_servers` sets the DNS server address(es) DHCP clients will use — an
array, so you can advertise more than one. This is typically the bridge IP
itself (if you're running a DNS forwarder there) or upstream resolvers.

```bash
# Advertise two upstream resolvers with a longer lease time
curl -s -X POST "$FABRIC_HOST/api/networkd/dhcp" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "bridge_name": "vm-bridge",
    "pool_start": "192.168.100.50",
    "pool_end": "192.168.100.149",
    "dns_servers": ["1.1.1.1", "8.8.8.8"],
    "lease_time_sec": 7200
  }' | jq .
```

---

## Step 8: Advanced Network Types

### TAP Interfaces

Create a standalone TAP interface for direct VM attachment:

```bash
curl -s -X POST "$FABRIC_HOST/api/networkd/taps" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "tap-web-01",
    "bridge": "vm-bridge",
    "mtu": 1500
  }' | jq .
```

### MACVTAP Interfaces

MACVTAP provides direct attachment to the physical NIC without a bridge:

```bash
curl -s -X POST "$FABRIC_HOST/api/networkd/macvtaps" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "macvtap-db",
    "parent_interface": "enp0s3",
    "mode": "bridge"
  }' | jq .
```

### VXLAN Tunnels

VXLAN extends Layer 2 networks across hosts for multi-node setups:

```bash
curl -s -X POST "$FABRIC_HOST/api/networkd/vxlans" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "vxlan100",
    "vni": 100,
    "remote": "10.0.0.2",
    "local": "10.0.0.1",
    "port": 4789,
    "mtu": 1450
  }' | jq .
```

---

## Complete Networking Example

Here is a real-world setup with a bridge, two VLANs, and network policies:

```bash
# 1. Create the main bridge
curl -s -X POST "$FABRIC_HOST/api/networkd/bridges" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "prod-bridge",
    "stp": true,
    "vlan_filtering": true,
    "addresses": ["172.16.0.1/16"],
    "dns": ["172.16.0.1"]
  }' | jq .id

# 2. Create app VLAN
curl -s -X POST "$FABRIC_HOST/api/networkd/vlans" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "vlan-app",
    "vlan_id": 10,
    "parent_interface": "prod-bridge",
    "addresses": ["172.16.10.1/24"]
  }' | jq .id

# 3. Create database VLAN
curl -s -X POST "$FABRIC_HOST/api/networkd/vlans" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "vlan-db",
    "vlan_id": 20,
    "parent_interface": "prod-bridge",
    "addresses": ["172.16.20.1/24"]
  }' | jq .id

# 4. Allow app -> db on port 5432 only
curl -s -X POST "$FABRIC_HOST/api/network-policies" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "app-to-db",
    "endpoint_selector": {"match_labels": {"tier": "database"}},
    "ingress": [{
      "ports": [{"protocol": "tcp", "port": 5432}],
      "from": [{"match_labels": {"tier": "app"}}]
    }],
    "egress": []
  }' | jq .id
```

---

## Cleanup

```bash
# Delete VLANs (use actual IDs from your responses)
curl -s -X DELETE "$FABRIC_HOST/api/networkd/vlans/$VLAN_APP_ID" \
  -H "Authorization: Bearer $TOKEN"
curl -s -X DELETE "$FABRIC_HOST/api/networkd/vlans/$VLAN_DB_ID" \
  -H "Authorization: Bearer $TOKEN"

# Delete bridge
curl -s -X DELETE "$FABRIC_HOST/api/networkd/bridges/$BRIDGE_ID" \
  -H "Authorization: Bearer $TOKEN"

# Delete network policies
curl -s -X DELETE "$FABRIC_HOST/api/network-policies/$POLICY_ID" \
  -H "Authorization: Bearer $TOKEN"
```

---

## Next Steps

- [Tutorial 03: Snapshots & Backups](03-snapshots-backups.md) -- Protect your VMs with snapshots and automated backups
- [Tutorial 05: Multi-Node Clustering](05-clustering.md) -- Use VXLAN and bridges across a cluster
- [Tutorial 06: Security Hardening](06-security-hardening.md) -- Firewall profiles and network isolation
- [Tutorial 09: VM Edge Dataplane](09-edge-dataplane.md) -- FluxVM schema v4 groups/CNP via Fabric (orthogonal to Step 5 SDN)
- [Edge dataplane series](edge-dataplane/README.md) -- Short labs for health, groups, CNP, observe, UX
