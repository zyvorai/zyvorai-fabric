# Tutorial 05: Multi-Node Clustering

Deploy VMs across multiple physical hosts with datacenters, clusters, resource
pools, live migration, high availability, and Distributed Resource Scheduling
(DRS). This tutorial covers the enterprise clustering features of Zyvor Fabric.

**Level:** Advanced
**Time:** 60 minutes
**Prerequisites:** Two or more hosts running Zyvor Fabric, network connectivity between hosts

---

## What You Will Learn

1. Create datacenters and clusters
2. Register and manage hosts
3. Configure resource pools with CPU and memory limits
4. Migrate VMs between hosts (live and offline)
5. Set up high availability
6. Configure DRS for automatic load balancing

---

## Architecture Overview

```
+--------------------------------------------------------------------+
|                          DATACENTER: us-east                        |
|                                                                    |
|  +--------------------------+  +--------------------------+        |
|  |    CLUSTER: production   |  |    CLUSTER: staging      |        |
|  |                          |  |                          |        |
|  |  +--------+ +--------+  |  |  +--------+              |        |
|  |  | host-01| | host-02|  |  |  | host-03|              |        |
|  |  | 64 CPU | | 64 CPU |  |  |  | 32 CPU |              |        |
|  |  | 256 GB | | 256 GB |  |  |  | 128 GB |              |        |
|  |  +---+----+ +---+----+  |  |  +---+----+              |        |
|  |      |          |       |  |      |                    |        |
|  |  +---+---+  +---+---+  |  |  +---+---+               |        |
|  |  | vm-01 |  | vm-03 |  |  |  | vm-05 |               |        |
|  |  | vm-02 |  | vm-04 |  |  |  | vm-06 |               |        |
|  |  +-------+  +-------+  |  |  +-------+               |        |
|  |                          |  |                          |        |
|  |  RESOURCE POOLS:         |  +--------------------------+        |
|  |  +----------+---------+  |                                      |
|  |  | web-tier | db-tier |  |                                      |
|  |  | 50% CPU  | 50% CPU |  |                                      |
|  |  +----------+---------+  |                                      |
|  +--------------------------+                                      |
+--------------------------------------------------------------------+
```

---

## Setup

```bash
export FABRIC_HOST="http://localhost:3000"
TOKEN=$(curl -s "$FABRIC_HOST/api/auth/login" \
  -H "Content-Type: application/json" \
  -d '{"username": "admin", "password": "your-password"}' | jq -r '.token')
```

---

## Step 1: Create a Datacenter

A datacenter is the top-level organizational unit. It groups clusters that share
a common network and storage fabric.

```bash
curl -s -X POST "$FABRIC_HOST/api/datacenters" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "us-east",
    "description": "US East Coast datacenter - Virginia"
  }' | jq .
```

Expected response:

```json
{
  "id": "dc-a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "name": "us-east",
  "description": "US East Coast datacenter - Virginia",
  "clusters": [],
  "created_at": "2026-04-12T14:00:00Z",
  "updated_at": "2026-04-12T14:00:00Z",
  "status": "active"
}
```

Save the datacenter ID:

```bash
DC_ID="dc-a1b2c3d4-e5f6-7890-abcd-ef1234567890"
```

### List Datacenters

```bash
curl -s "$FABRIC_HOST/api/datacenters" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

### Get Datacenter Details

```bash
curl -s "$FABRIC_HOST/api/datacenters/$DC_ID" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

### Update a Datacenter

```bash
curl -s -X PUT "$FABRIC_HOST/api/datacenters/$DC_ID" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "description": "US East Coast datacenter - Virginia (primary)"
  }' | jq .
```

---

## Step 2: Create Clusters

A cluster groups hosts that can share VMs via live migration. All hosts in a
cluster should have compatible CPU architectures.

### Production Cluster

```bash
curl -s -X POST "$FABRIC_HOST/api/datacenters/$DC_ID/clusters" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "production",
    "description": "Production workloads - HA enabled",
    "ha_enabled": true,
    "drs_enabled": true
  }' | jq .
```

Expected response:

```json
{
  "id": "cl-b2c3d4e5-f6a7-8901-bcde-f23456789012",
  "name": "production",
  "description": "Production workloads - HA enabled",
  "datacenter_id": "dc-a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "ha_enabled": true,
  "drs_enabled": true,
  "hosts": [],
  "created_at": "2026-04-12T14:05:00Z",
  "updated_at": "2026-04-12T14:05:00Z"
}
```

```bash
CLUSTER_ID="cl-b2c3d4e5-f6a7-8901-bcde-f23456789012"
```

### Staging Cluster

```bash
curl -s -X POST "$FABRIC_HOST/api/datacenters/$DC_ID/clusters" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "staging",
    "description": "Pre-production testing",
    "ha_enabled": false,
    "drs_enabled": false
  }' | jq .
```

### List Clusters

```bash
curl -s "$FABRIC_HOST/api/datacenters/$DC_ID/clusters" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

---

## Step 3: Register Hosts

Register physical hypervisor hosts into a cluster. Each host runs its own
Zyvor Fabric instance.

### Register Host 01

```bash
curl -s -X POST "$FABRIC_HOST/api/hosts" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "hostname": "host-01.example.com",
    "address": "10.0.1.10",
    "cluster_id": "'"$CLUSTER_ID"'",
    "cpu_cores": 64,
    "memory_mb": 262144,
    "tags": ["ssd", "gpu"]
  }' | jq .
```

Expected response:

```json
{
  "id": "host-c3d4e5f6-a7b8-9012-cdef-345678901234",
  "hostname": "host-01.example.com",
  "address": "10.0.1.10",
  "cluster_id": "cl-b2c3d4e5-f6a7-8901-bcde-f23456789012",
  "status": "online",
  "cpu_cores": 64,
  "memory_mb": 262144,
  "tags": ["ssd", "gpu"],
  "registered_at": "2026-04-12T14:10:00Z"
}
```

### Register Host 02

```bash
curl -s -X POST "$FABRIC_HOST/api/hosts" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "hostname": "host-02.example.com",
    "address": "10.0.1.11",
    "cluster_id": "'"$CLUSTER_ID"'",
    "cpu_cores": 64,
    "memory_mb": 262144,
    "tags": ["ssd"]
  }' | jq .
```

### Host Heartbeats

Hosts send periodic heartbeats to report their current state. The API exposes
this data:

```bash
curl -s "$FABRIC_HOST/api/hosts/host-c3d4e5f6-a7b8-9012-cdef-345678901234/heartbeat" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

Expected response:

```json
{
  "host_id": "host-c3d4e5f6-...",
  "cpu_usage_percent": 23.5,
  "memory_used_mb": 102400,
  "memory_free_mb": 159744,
  "vm_count": 12,
  "uptime_seconds": 864000,
  "last_seen": "2026-04-12T14:11:00Z"
}
```

### List Hosts

```bash
curl -s "$FABRIC_HOST/api/hosts" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

---

## Step 4: Resource Pools

Resource pools partition cluster resources among groups of VMs. They enforce CPU
and memory limits to prevent any single workload from starving others.

### Create a Web Tier Pool

```bash
curl -s -X POST "$FABRIC_HOST/api/resource-pools" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "web-tier",
    "cluster_id": "'"$CLUSTER_ID"'",
    "cpu_shares": 4000,
    "cpu_reservation_mhz": 8000,
    "cpu_limit_mhz": 32000,
    "cpu_expandable_reservation": true,
    "memory_shares": 4000,
    "memory_reservation_mb": 16384,
    "memory_limit_mb": 65536,
    "memory_expandable_reservation": true
  }' | jq .
```

Expected response:

```json
{
  "id": "rp-d4e5f6a7-b8c9-0123-defg-456789012345",
  "name": "web-tier",
  "cluster_id": "cl-b2c3d4e5-...",
  "cpu_shares": 4000,
  "cpu_reservation_mhz": 8000,
  "cpu_limit_mhz": 32000,
  "cpu_expandable_reservation": true,
  "memory_shares": 4000,
  "memory_reservation_mb": 16384,
  "memory_limit_mb": 65536,
  "memory_expandable_reservation": true,
  "vms": [],
  "children": [],
  "created": "2026-04-12T14:15:00Z"
}
```

### Create a Database Tier Pool

```bash
curl -s -X POST "$FABRIC_HOST/api/resource-pools" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "db-tier",
    "cluster_id": "'"$CLUSTER_ID"'",
    "cpu_shares": 8000,
    "cpu_reservation_mhz": 16000,
    "cpu_limit_mhz": 64000,
    "memory_shares": 8000,
    "memory_reservation_mb": 65536,
    "memory_limit_mb": 131072
  }' | jq .
```

### Resource Pool Parameters

| Parameter                        | Type    | Description                          |
|---------------------------------|---------|--------------------------------------|
| `name`                          | string  | Pool name                            |
| `cluster_id`                    | string  | Parent cluster                       |
| `parent_id`                     | string  | Parent pool (for nesting)            |
| `cpu_shares`                    | integer | Relative CPU allocation weight       |
| `cpu_reservation_mhz`          | integer | Guaranteed CPU in MHz                |
| `cpu_limit_mhz`                | integer | Maximum CPU in MHz                   |
| `cpu_expandable_reservation`   | bool    | Allow borrowing from parent          |
| `memory_shares`                 | integer | Relative memory allocation weight    |
| `memory_reservation_mb`        | integer | Guaranteed memory in MB              |
| `memory_limit_mb`              | integer | Maximum memory in MB                 |
| `memory_expandable_reservation`| bool    | Allow borrowing from parent          |

### List Resource Pools

```bash
curl -s "$FABRIC_HOST/api/resource-pools" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

### Admission Control

Check whether a cluster can accommodate a new VM before creating it:

```bash
curl -s -X POST "$FABRIC_HOST/api/resource-pools/$POOL_ID/admission-check" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "cpu_mhz": 4000,
    "memory_mb": 8192
  }' | jq .
```

Expected response:

```json
{
  "admitted": true,
  "available_cpu_mhz": 28000,
  "available_memory_mb": 57344
}
```

---

## Step 5: VM Migration

Move VMs between hosts within a cluster. Zyvor Fabric supports **disk-copy**
live/offline/storage migration (`/api/migrations`, rsync over SSH) and a
**native** FluxVM path (`…/migration/native/*`, preview — see
[FLUXVM-FABRIC-BOUNDARY.md](../FLUXVM-FABRIC-BOUNDARY.md)).

### Live Migration (disk-copy)

Migrate a running VM to another host with a brief final-sync pause (rsync, not QEMU memory migration):

```bash
curl -s -X POST "$FABRIC_HOST/api/migrations" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "vm_name": "web-server-01",
    "target_host": "host-02.example.com",
    "migration_type": "live",
    "compress": true,
    "bandwidth_mbps": 1000
  }' | jq .
```

Expected response:

```json
{
  "id": "mig-e5f6a7b8-c9d0-1234-efgh-567890123456",
  "vm_name": "web-server-01",
  "target_host": "host-02.example.com",
  "migration_type": "live",
  "state": "pending",
  "progress_percent": 0,
  "bytes_transferred": 0,
  "started": "2026-04-12T14:20:00Z",
  "completed": null,
  "error": null
}
```

### Monitor Migration Progress

```bash
curl -s "$FABRIC_HOST/api/migrations/mig-e5f6a7b8-c9d0-1234-efgh-567890123456" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

Expected response during migration:

```json
{
  "id": "mig-e5f6a7b8-...",
  "vm_name": "web-server-01",
  "target_host": "host-02.example.com",
  "migration_type": "live",
  "state": "syncing",
  "progress_percent": 67,
  "bytes_transferred": 1717986918,
  "started": "2026-04-12T14:20:00Z",
  "completed": null,
  "error": null
}
```

### Migration States

```
pending --> pre_check --> syncing --> switching --> completed
                |            |           |
                +------------+-----------+---> failed
                                              cancelled
```

| State      | Description                                        |
|-----------|----------------------------------------------------|
| `pending`  | Migration is queued                                |
| `pre_check`| Validating target host compatibility               |
| `syncing`  | Copying memory pages to target                     |
| `switching`| Final switchover (brief pause for live migration)  |
| `completed`| VM is running on the target host                   |
| `failed`   | Migration failed; VM remains on source host        |
| `cancelled`| Migration was cancelled by the operator            |

### Migration Types

| Type      | Description                                         |
|----------|------------------------------------------------------|
| `live`   | Memory and state transferred while VM runs; brief pause at switchover |
| `offline`| VM is stopped, disk is copied, VM is started on target |
| `storage`| Only the disk is migrated; VM stays on the same host  |

### Migration Parameters

| Parameter       | Type    | Description                          |
|----------------|---------|--------------------------------------|
| `vm_name`      | string  | VM to migrate                        |
| `target_host`  | string  | Destination hostname or IP           |
| `migration_type`| string | `live`, `offline`, or `storage`      |
| `compress`     | bool    | Compress memory pages during transfer|
| `bandwidth_mbps`| integer| Bandwidth limit for migration traffic|

### List Migrations

```bash
curl -s "$FABRIC_HOST/api/migrations" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

### Cancel a Migration

```bash
curl -s -X POST "$FABRIC_HOST/api/migrations/mig-e5f6a7b8-c9d0-1234-efgh-567890123456/cancel" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

---

## Step 6: High Availability

When HA is enabled on a cluster, Zyvor Fabric monitors hosts and automatically
restarts VMs on surviving hosts if a host fails.

### HA Architecture

```
                    +------------------+
                    |  HA Controller   |
                    | (Zyvor Fabric leader)|
                    +--------+---------+
                             |
              +--------------+--------------+
              |              |              |
         +----+----+   +----+----+   +----+----+
         | host-01 |   | host-02 |   | host-03 |
         | (active)|   | (active)|   | (active)|
         +---------+   +---------+   +---------+
              |              |              |
         heartbeat      heartbeat      heartbeat
```

If `host-01` fails:

```
         +----+----+   +----+----+   +----+----+
         | host-01 |   | host-02 |   | host-03 |
         |  (DOWN) |   | (active)|   | (active)|
         +---------+   +----+----+   +----+----+
                             |              |
                        vm-01, vm-02   (auto-restart)
                        restarted here
```

### Enable HA on a Cluster

HA is configured at the cluster level:

```bash
curl -s -X PUT "$FABRIC_HOST/api/datacenters/$DC_ID/clusters/$CLUSTER_ID" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "ha_enabled": true
  }' | jq .
```

---

## Step 7: Distributed Resource Scheduling (DRS)

DRS automatically balances VM workloads across hosts in a cluster. It monitors
CPU and memory usage and recommends (or automatically executes) migrations to
prevent hotspots.

### Configure DRS

```bash
curl -s -X POST "$FABRIC_HOST/api/drs/configure" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "cluster_id": "'"$CLUSTER_ID"'",
    "enabled": true,
    "automation_level": "fully_automated",
    "target_imbalance_threshold": 0.25,
    "migration_threshold": 3
  }' | jq .
```

Expected response:

```json
{
  "cluster_id": "cl-b2c3d4e5-...",
  "enabled": true,
  "automation_level": "fully_automated",
  "target_imbalance_threshold": 0.25,
  "migration_threshold": 3
}
```

### DRS Automation Levels

| Level              | Description                                    |
|-------------------|------------------------------------------------|
| `manual`          | Generate recommendations; admin approves each  |
| `partially_automated` | Auto-place new VMs; manual rebalancing    |
| `fully_automated` | Auto-place and auto-migrate VMs                |

### Get DRS Recommendations

Ask DRS to analyze current cluster state and suggest migrations:

```bash
curl -s -X POST "$FABRIC_HOST/api/drs/recommendations" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "cluster_id": "'"$CLUSTER_ID"'",
    "hosts": [
      {
        "host_id": "host-01",
        "cpu_usage_percent": 85.0,
        "memory_usage_percent": 72.0,
        "vm_count": 15
      },
      {
        "host_id": "host-02",
        "cpu_usage_percent": 30.0,
        "memory_usage_percent": 25.0,
        "vm_count": 5
      }
    ],
    "vms": [
      {
        "vm_name": "heavy-workload",
        "host_id": "host-01",
        "cpu_usage_mhz": 8000,
        "memory_usage_mb": 16384
      }
    ]
  }' | jq .
```

Expected response:

```json
[
  {
    "vm_name": "heavy-workload",
    "source_host": "host-01",
    "target_host": "host-02",
    "reason": "CPU imbalance: host-01 at 85%, host-02 at 30%",
    "priority": "high",
    "estimated_benefit": {
      "source_cpu_after": 55.0,
      "target_cpu_after": 55.0
    }
  }
]
```

### Analyze Cluster Balance

Check the current balance across all hosts:

```bash
curl -s -X POST "$FABRIC_HOST/api/drs/clusters/$CLUSTER_ID/balance" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "hosts": [
      {"host_id": "host-01", "cpu_usage_percent": 85.0, "memory_usage_percent": 72.0, "vm_count": 15},
      {"host_id": "host-02", "cpu_usage_percent": 30.0, "memory_usage_percent": 25.0, "vm_count": 5}
    ]
  }' | jq .
```

### VM Placement

When creating a new VM, ask DRS where to place it:

```bash
curl -s -X POST "$FABRIC_HOST/api/drs/placement" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "vm_name": "new-web-server",
    "cpu_mhz": 4000,
    "memory_mb": 8192,
    "affinity_rules": [
      {
        "type": "anti_affinity",
        "vm_group": ["web-server-01", "web-server-02"],
        "rule": "should"
      }
    ]
  }' | jq .
```

Expected response:

```json
{
  "recommended_host": "host-02",
  "reason": "Lowest resource utilization; anti-affinity with web-server group satisfied",
  "alternatives": ["host-03"]
}
```

### Affinity Rules

| Type            | Description                                    |
|----------------|------------------------------------------------|
| `affinity`     | Place VM on the same host as the specified group |
| `anti_affinity`| Place VM on a different host from the group    |

Rule strength:
- `"must"` -- hard constraint; placement fails if unsatisfied
- `"should"` -- soft constraint; preferred but not required

---

## Complete Cluster Topology

```
Datacenter: us-east
  +-- Cluster: production (HA, DRS)
  |     +-- Host: host-01 (64 CPU, 256GB)
  |     |     +-- vm-web-01
  |     |     +-- vm-web-02
  |     |     +-- vm-db-01
  |     +-- Host: host-02 (64 CPU, 256GB)
  |           +-- vm-web-03
  |           +-- vm-db-02
  |
  +-- Resource Pools
  |     +-- web-tier (4000 shares, 8GHz reserved)
  |     +-- db-tier  (8000 shares, 16GHz reserved)
  |
  +-- Cluster: staging
        +-- Host: host-03 (32 CPU, 128GB)
              +-- vm-staging-01
              +-- vm-staging-02
```

---

## Cleanup

```bash
# Delete clusters and datacenters (use actual IDs)
curl -s -X DELETE "$FABRIC_HOST/api/datacenters/$DC_ID/clusters/$CLUSTER_ID" \
  -H "Authorization: Bearer $TOKEN"

curl -s -X DELETE "$FABRIC_HOST/api/datacenters/$DC_ID" \
  -H "Authorization: Bearer $TOKEN"
```

---

## Next Steps

- [Tutorial 06: Security Hardening](06-security-hardening.md) -- Secure your cluster with encryption and access control
- [Tutorial 02: VM Networking](02-networking.md) -- Configure VXLAN overlays for cross-host networking
