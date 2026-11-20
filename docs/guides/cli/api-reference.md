# REST API Reference

Complete reference for the Zyvor Fabric REST API, organized by functional category. All endpoints require JWT authentication unless noted otherwise.

**Base URL:** `http://<host>:3000`

## Table of Contents

- [Authentication](#authentication)
- [Virtual Machines](#virtual-machines)
- [Images](#images)
- [Snapshots](#snapshots)
- [Backups](#backups)
- [Networking](#networking)
- [VM edge dataplane (Network Fabric)](#vm-edge-dataplane-network-fabric)
- [Service Fabric (Maglev VIP LB)](#service-fabric-v6-bpf-schema-4-maglev-vip-lb)
- [Storage](#storage)
- [Machines (VM driver)](#machines-vm-driver)
- [Events](#events)
- [System](#system)
- [Cloud-init](#cloud-init)
- [Notifications](#notifications)
- [WebSocket Console](#websocket-console)
- [2FA Authentication](#2fa-authentication)
- [Secrets Management](#secrets-management)
- [Log Aggregation](#log-aggregation)
- [VM Export](#vm-export)
- [Compliance Scanning](#compliance-scanning)
- [Billing](#billing)
- [iSCSI Storage](#iscsi-storage)
- [USB Devices](#usb-devices)
- [DHCP Server](#dhcp-server)

---

## Authentication

### POST /api/auth/login

Authenticate with system credentials via PAM and receive a JWT token.

**Auth level:** None (public endpoint)

**Rate limits:**
- Per-user: 5 failed attempts per 5-minute window
- Global: 50 failed attempts per 5-minute window across all users

**Request body:**

```json
{
  "username": "admin",
  "password": "secret"
}
```

Username constraints: 1-64 characters, ASCII alphanumeric plus `-`, `_`, `.` only.

**Response (200):**

```json
{
  "token": "eyJhbGciOiJIUzI1NiJ9...",
  "user_id": "admin",
  "role": "admin",
  "username": "admin"
}
```

**Roles assigned:**
- `admin` -- root user, or members of `wheel`, `sudo`, or `adm` groups
- `user` -- all other authenticated system users

**Error responses:**
- `400` -- Invalid username format
- `401` -- Invalid credentials
- `429` -- Rate limit exceeded

**curl example:**

```bash
curl -s -X POST http://localhost:3000/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"secret"}' | jq

# Store the token for subsequent requests
TOKEN=$(curl -s -X POST http://localhost:3000/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"secret"}' | jq -r '.token')
```

---

### GET /api/auth/me

Return the authenticated user's profile.

**Auth level:** Any authenticated user (Viewer+)

**Response (200):**

```json
{
  "id": "admin",
  "username": "admin",
  "role": "admin"
}
```

**curl example:**

```bash
curl -s http://localhost:3000/api/auth/me \
  -H "Authorization: Bearer $TOKEN" | jq
```

---

## Virtual Machines

### GET /api/vms

List all VMs with pagination.

**Auth level:** Viewer+

**Query parameters:**

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `offset`  | int  | 0       | Starting index |
| `limit`   | int  | 200     | Max results (capped at 1000) |
| `tenant`  | string | —     | Filter by `labels.tenant` or tag `tenant:…` |

**Response (200):**

```json
{
  "items": [ ... ],
  "total": 15,
  "offset": 0,
  "limit": 200
}
```

`fabricctl list` decodes this paginated envelope (`items`, not a bare array).

```bash
curl -s "http://localhost:9095/api/vms?tenant=acme" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

**curl example:**

```bash
curl -s "http://localhost:3000/api/vms?offset=0&limit=10" \
  -H "Authorization: Bearer $TOKEN" | jq
```

---

### GET /api/vms/:name

Get a single VM by name.

**Auth level:** Viewer+

**Response (200):** VM object

**Error responses:**
- `400` -- Invalid VM name
- `404` -- VM not found

**curl example:**

```bash
curl -s http://localhost:3000/api/vms/web-server \
  -H "Authorization: Bearer $TOKEN" | jq
```

---

### POST /api/vms

Create a new VM.

**Auth level:** User+

**Request body:**

```json
{
  "name": "my-vm",
  "image": "/var/lib/fluxvm/images/ubuntu.qcow2",
  "cpus": 2,
  "memory": 2048,
  "disk": 20,
  "tenant": "acme",
  "labels": {
    "env": "dev",
    "team": "platform"
  }
}
```

Optional `tenant` is merged into `labels["tenant"]` (existing label wins). On
start, Fabric passes that value to FluxVM’s first-class `CreateVmRequest.tenant`.

VM name constraints: alphanumeric plus `-` and `_`, validated server-side.

On create, Fabric provisions a disk image under its storage path (`cp --reflink=auto`
from `image` when the source exists, otherwise `qemu-img create`) so clone/start
have a backing file. Failures are logged but do not fail the create.

**Response (201):** Created VM object

**Error responses:**
- `400` -- Invalid name or parameters
- `409` -- VM with this name already exists

**curl example:**

```bash
curl -s -X POST http://localhost:9095/api/vms \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "my-vm",
    "image": "/var/lib/fluxvm/images/ubuntu.qcow2",
    "cpus": 2,
    "memory": 2048,
    "disk": 20,
    "tenant": "acme"
  }' | jq
```

---

### DELETE /api/vms/:name

Delete a VM. Removes the VM record and deallocates its security identity.

**Auth level:** Admin only

**Response (204):** No content

**curl example:**

```bash
curl -s -X DELETE http://localhost:3000/api/vms/my-vm \
  -H "Authorization: Bearer $TOKEN"
```

---

### POST /api/vms/:name/start

Start a VM. Returns immediately with `202 Accepted`; the VM transitions through `Starting` to `Running` in the background.

**Auth level:** User+

**Request body (optional):** `VMStartOptions` — low-level launch options. FluxVM honors `network_tap`, `bind_mounts` (translated into virtiofs shares), `linux`/`initrd`/`firmware`, and `extra_args`; fields with no FluxVM equivalent (`tpm`, `secure_boot`, `vsock`, `credentials`, `directory`, `extra_drives`, `bind_users`) fail with a clear "not supported" error rather than being silently dropped.

```json
{
  "network_tap": true,
  "bind_mounts": [
    { "source": "/data", "destination": "/mnt/data", "read_only": false }
  ]
}
```

Without a body, the VM is started as previously configured.

**Response (202):**

```json
{
  "status": "starting"
}
```

**Error responses:**
- `400` -- Invalid start options
- `409` -- VM is already running, starting, or stopping

**curl example:**

```bash
# Default start
curl -s -X POST http://localhost:3000/api/vms/my-vm/start \
  -H "Authorization: Bearer $TOKEN" | jq

# Start with options
curl -s -X POST http://localhost:3000/api/vms/my-vm/start \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"console": true}' | jq
```

---

### POST /api/vms/:name/stop

Gracefully power off a VM via the active VM driver.

**Auth level:** User+

**Response (200):**

```json
{
  "status": "stopped"
}
```

**curl example:**

```bash
curl -s -X POST http://localhost:3000/api/vms/my-vm/stop \
  -H "Authorization: Bearer $TOKEN" | jq
```

---

### POST /api/vms/:name/restart

Reboot a running VM.

**Auth level:** User+

**Response (200):**

```json
{
  "status": "restarted"
}
```

**curl example:**

```bash
curl -s -X POST http://localhost:3000/api/vms/my-vm/restart \
  -H "Authorization: Bearer $TOKEN" | jq
```

---

### POST /api/vms/:name/pause

Pause (freeze) a running VM.

**Auth level:** User+

**Response (200):**

```json
{
  "status": "paused"
}
```

**curl example:**

```bash
curl -s -X POST http://localhost:3000/api/vms/my-vm/pause \
  -H "Authorization: Bearer $TOKEN" | jq
```

---

### POST /api/vms/:name/resume

Resume a paused VM.

**Auth level:** User+

**Response (200):**

```json
{
  "status": "running"
}
```

**curl example:**

```bash
curl -s -X POST http://localhost:3000/api/vms/my-vm/resume \
  -H "Authorization: Bearer $TOKEN" | jq
```

---

### POST /api/vms/:name/clone

Clone a VM by duplicating its disk image and creating a new VM record. Source disk
is resolved from the FluxVM driver path or Fabric storage when the VM was created
via `POST /api/vms` (disk provisioned on create).

**Auth level:** User+

**Request body:**

```json
{
  "target_name": "my-vm-clone",
  "linked_clone": false
}
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `target_name` | string | required | Name for the cloned VM |
| `linked_clone` | bool | false | If true, create a qcow2 backing-file clone (source must be stopped) |

**Response (201):** Cloned VM object (state: `stopped`)

**Error responses:**
- `400` -- Same source and target name
- `404` -- Source VM or disk image not found
- `409` -- Target name already exists; linked clone while source is running

**curl example:**

```bash
curl -s -X POST http://localhost:3000/api/vms/my-vm/clone \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"target_name": "my-vm-clone", "linked_clone": false}' | jq
```

---

### GET /api/vms/:name/metrics

Get runtime metrics for a VM.

**Auth level:** Viewer+

Returns **404** only when the VM is absent from Fabric's store. When the VM exists
in the store but is not registered in FluxVM (metadata-only / stopped), returns
**200** with zeroed counters instead of an error.

**Response (200):**

```json
{
  "cpu_usage": 0.0,
  "memory_usage": 0,
  "disk_usage": 0,
  "network_rx": 0,
  "network_tx": 0
}
```

**curl example:**

```bash
curl -s http://localhost:3000/api/vms/my-vm/metrics \
  -H "Authorization: Bearer $TOKEN" | jq
```

---

## Images

### POST /api/images/build

Build a new VM image using mkosi.

**Auth level:** User+

**Request body:**

```json
{
  "name": "fedora-base",
  "distribution": "fedora",
  "packages": ["vim", "htop", "nginx"],
  "autologin": true
}
```

Allowed distributions: `fedora`, `ubuntu`, `debian`, `centos`, `arch`, `opensuse`, `alma`, `rocky`.

**Response (201):**

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "name": "fedora-base",
  "distribution": "fedora",
  "state": "pending",
  "output_path": null,
  "error": null,
  "started": "2026-04-12T10:00:00Z",
  "completed": null
}
```

Build states: `pending` -> `building` -> `completed` | `failed`

**curl example:**

```bash
curl -s -X POST http://localhost:3000/api/images/build \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "fedora-base",
    "distribution": "fedora",
    "packages": ["vim", "htop"],
    "autologin": true
  }' | jq
```

---

### GET /api/images

List all available VM images.

**Auth level:** Viewer+

**curl example:**

```bash
curl -s http://localhost:3000/api/images \
  -H "Authorization: Bearer $TOKEN" | jq
```

---

### GET /api/images/:id

Get details of a specific image or build status.

**Auth level:** Viewer+

**curl example:**

```bash
curl -s http://localhost:3000/api/images/550e8400-e29b-41d4-a716-446655440000 \
  -H "Authorization: Bearer $TOKEN" | jq
```

---

### POST /api/images/download

Download a cloud image from a distribution's official repository.

**Auth level:** User+

**Request body:**

```json
{
  "distribution": "fedora",
  "version": "41",
  "format": "qcow2"
}
```

**curl example:**

```bash
curl -s -X POST http://localhost:3000/api/images/download \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"distribution":"fedora","version":"41","format":"qcow2"}' | jq
```

---

### POST /api/images/import

Import an existing disk image from a local path.

**Auth level:** Admin only

**Request body:**

```json
{
  "name": "custom-image",
  "path": "/tmp/custom.qcow2",
  "format": "qcow2"
}
```

**curl example:**

```bash
curl -s -X POST http://localhost:3000/api/images/import \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name":"custom-image","path":"/tmp/custom.qcow2","format":"qcow2"}' | jq
```

---

### POST /api/images/:id/resize

Resize an image's virtual disk.

**Auth level:** User+

**Request body:**

```json
{
  "size_gb": 40
}
```

**curl example:**

```bash
curl -s -X POST http://localhost:3000/api/images/550e8400/resize \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"size_gb": 40}' | jq
```

---

### DELETE /api/images/:id

Delete an image.

**Auth level:** Admin only

**Response (204):** No content

**curl example:**

```bash
curl -s -X DELETE http://localhost:3000/api/images/550e8400 \
  -H "Authorization: Bearer $TOKEN"
```

---

### GET /api/images/iso

List available ISO images.

**Auth level:** Viewer+

**curl example:**

```bash
curl -s http://localhost:3000/api/images/iso \
  -H "Authorization: Bearer $TOKEN" | jq
```

---

## Snapshots

### POST /api/vms/:name/snapshots

Create a snapshot of a VM.

**Auth level:** User+

**Request body:**

```json
{
  "name": "before-upgrade",
  "description": "Snapshot before OS upgrade",
  "snapshot_type": "Disk"
}
```

Snapshot types:

| Type | Default | Running / paused VM | Stopped VM |
|------|---------|---------------------|------------|
| `Disk` | yes | QMP `blockdev-snapshot-internal-sync` (fast, disk only) | `qemu-img snapshot -c` |
| `Full` | no | QMP `snapshot-save` (disk + memory; may take minutes) | `qemu-img snapshot -c` |

**Response (201):**

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "vm_name": "my-vm",
  "name": "before-upgrade",
  "description": "Snapshot before OS upgrade",
  "snapshot_type": "Disk",
  "parent_id": null,
  "size_bytes": 1073741824,
  "created": "2026-04-12T10:00:00Z"
}
```

**Errors:**

- **409** — live snapshot could not reach the QMP monitor yet (VM still starting). Wait and retry.
- **500** — QEMU/job failure on `Full`, or `qemu-img` failure on a stopped VM.

**curl example:**

```bash
curl -s -X POST http://localhost:3000/api/vms/my-vm/snapshots \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name":"before-upgrade","description":"Pre-upgrade snapshot","snapshot_type":"Disk"}' | jq
```

---

### GET /api/vms/:name/snapshots

List all snapshots for a VM.

**Auth level:** Viewer+

**Response (200):** Array of snapshot objects

**curl example:**

```bash
curl -s http://localhost:3000/api/vms/my-vm/snapshots \
  -H "Authorization: Bearer $TOKEN" | jq
```

---

### GET /api/vms/:name/snapshots/:id

Get a specific snapshot.

**Auth level:** Viewer+

**curl example:**

```bash
curl -s http://localhost:3000/api/vms/my-vm/snapshots/550e8400 \
  -H "Authorization: Bearer $TOKEN" | jq
```

---

### DELETE /api/vms/:name/snapshots/:id

Delete a snapshot.

**Auth level:** Admin only

**Response (204):** No content

**curl example:**

```bash
curl -s -X DELETE http://localhost:3000/api/vms/my-vm/snapshots/550e8400 \
  -H "Authorization: Bearer $TOKEN"
```

---

### POST /api/vms/:name/snapshots/:id/revert

Revert a VM to a specific snapshot.

**Auth level:** User+

**Response (200):**

```json
{
  "status": "reverted",
  "snapshot_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**curl example:**

```bash
curl -s -X POST http://localhost:3000/api/vms/my-vm/snapshots/550e8400/revert \
  -H "Authorization: Bearer $TOKEN" | jq
```

---

### GET /api/vms/:name/snapshots/tree

Get the snapshot tree hierarchy for a VM, showing parent-child relationships.

**Auth level:** Viewer+

**Response (200):**

```json
[
  {
    "snapshot": {
      "id": "aaa",
      "name": "initial",
      "parent_id": null
    },
    "children": [
      {
        "snapshot": {
          "id": "bbb",
          "name": "after-install",
          "parent_id": "aaa"
        },
        "children": []
      }
    ]
  }
]
```

**curl example:**

```bash
curl -s http://localhost:3000/api/vms/my-vm/snapshots/tree \
  -H "Authorization: Bearer $TOKEN" | jq
```

---

## Backups

### POST /api/backups

Create a new backup.

**Auth level:** User+

**Request body:**

```json
{
  "vm_name": "my-vm",
  "backup_type": "full",
  "compress": true,
  "retention_days": 30,
  "description": "Weekly full backup"
}
```

Backup types: `full`, `incremental`.

**Response (201):**

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "vm_name": "my-vm",
  "backup_type": "full",
  "size_bytes": 2147483648,
  "compressed": true,
  "created": "2026-04-12T10:00:00Z",
  "status": "completed",
  "storage_location": "/var/lib/zyvor-fabricd/backups/my-vm/550e8400.tar.zst",
  "retention_days": 30,
  "expires_at": "2026-05-12T10:00:00Z"
}
```

**curl example:**

```bash
curl -s -X POST http://localhost:3000/api/backups \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "vm_name": "my-vm",
    "backup_type": "full",
    "compress": true,
    "retention_days": 30
  }' | jq
```

---

### GET /api/backups

List all backups, optionally filtered by VM.

**Auth level:** Viewer+

**Query parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `vm`      | string | Filter by VM name |

**curl example:**

```bash
# All backups
curl -s http://localhost:3000/api/backups \
  -H "Authorization: Bearer $TOKEN" | jq

# Backups for a specific VM
curl -s "http://localhost:3000/api/backups?vm=my-vm" \
  -H "Authorization: Bearer $TOKEN" | jq
```

---

### GET /api/backups/:id

Get details of a specific backup.

**Auth level:** Viewer+

**curl example:**

```bash
curl -s http://localhost:3000/api/backups/550e8400 \
  -H "Authorization: Bearer $TOKEN" | jq
```

---

### DELETE /api/backups/:id

Delete a backup.

**Auth level:** Admin only

**Response (204):** No content

**curl example:**

```bash
curl -s -X DELETE http://localhost:3000/api/backups/550e8400 \
  -H "Authorization: Bearer $TOKEN"
```

---

### POST /api/backups/restore

Restore a VM from a backup.

**Auth level:** User+

**Request body:**

```json
{
  "backup_id": "550e8400-e29b-41d4-a716-446655440000",
  "target_vm_name": "my-vm-restored",
  "restore_config": true,
  "restore_disks": true,
  "restore_state": false
}
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `backup_id` | string | required | ID of the backup to restore |
| `target_vm_name` | string | null | New VM name (null = restore in place) |
| `restore_config` | bool | true | Restore VM configuration |
| `restore_disks` | bool | true | Restore disk images |
| `restore_state` | bool | false | Restore memory state (if available) |

**curl example:**

```bash
curl -s -X POST http://localhost:3000/api/backups/restore \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "backup_id": "550e8400-e29b-41d4-a716-446655440000",
    "target_vm_name": "my-vm-restored"
  }' | jq
```

---

### GET /api/backups/stats

Get aggregate backup statistics.

**Auth level:** Viewer+

**Response (200):**

```json
{
  "total_backups": 42,
  "total_size_bytes": 107374182400,
  "by_type": {"full": 30, "incremental": 12},
  "by_vm": {"web-server": 10, "db-server": 15},
  "oldest_backup": "2026-01-01T00:00:00Z",
  "newest_backup": "2026-04-12T10:00:00Z"
}
```

**curl example:**

```bash
curl -s http://localhost:3000/api/backups/stats \
  -H "Authorization: Bearer $TOKEN" | jq
```

---

### POST /api/backups/policies

Create a backup policy for automated scheduled backups.

**Auth level:** Admin only

**Request body:**

```json
{
  "name": "daily-full",
  "vm_tags": ["production"],
  "schedule_type": "daily",
  "backup_type": "full",
  "retention_days": 30,
  "enabled": true
}
```

Schedule types: `daily`, `weekly`, `monthly`.

**Response (201):** Backup policy object

**curl example:**

```bash
curl -s -X POST http://localhost:3000/api/backups/policies \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "daily-full",
    "vm_tags": ["production"],
    "schedule_type": "daily",
    "backup_type": "full",
    "retention_days": 30,
    "enabled": true
  }' | jq
```

---

### GET /api/backups/policies

List all backup policies.

**Auth level:** Viewer+

**curl example:**

```bash
curl -s http://localhost:3000/api/backups/policies \
  -H "Authorization: Bearer $TOKEN" | jq
```

---

### GET /api/backups/jobs

List backup jobs (running and completed).

**Auth level:** Viewer+

**Response (200):** Array of backup job objects with fields: `id`, `backup_id`, `vm_name`, `operation` (backup|restore), `status` (queued|running|completed|failed), `progress` (0.0-1.0).

**curl example:**

```bash
curl -s http://localhost:3000/api/backups/jobs \
  -H "Authorization: Bearer $TOKEN" | jq
```

---

## VM edge dataplane (Network Fabric)

FluxVM TC/eBPF edge proxied by Fabric. Orthogonal to host SDN
`/api/network-policies`. Schema **v4**. Operator guide:
[fluxvm-dataplane.md](../vm-drivers/fluxvm-dataplane.md).

**Auth:** Viewer+ for reads; Admin for writes (`POST`/`DELETE`).

### Per-VM

| Method | Path | Role |
|--------|------|------|
| GET | `/api/vms/{name}/dataplane/status` | mode, attached, schema_version, policy snapshot |
| GET/POST | `/api/vms/{name}/dataplane/policy` | durable `VmNetworkPolicy` |
| GET | `/api/vms/{name}/dataplane/effective` | declared + membership + merged |
| GET | `/api/vms/{name}/dataplane/stats` | allow/drop counters |
| GET | `/api/vms/{name}/dataplane/flows?limit=` | LRU flows |

### Cluster

| Method | Path | Role |
|--------|------|------|
| GET/POST | `/api/dataplane/groups` | list / upsert `SecurityGroup` |
| GET/DELETE | `/api/dataplane/groups/{name}` | get / delete |
| GET/POST | `/api/dataplane/cnp` | list / apply CNP JSON |
| GET/DELETE | `/api/dataplane/cnp/{name}` | get / delete |
| GET | `/api/dataplane/identities` | reserved + group identities |
| GET | `/api/dataplane/observe` | snapshot |
| GET | `/api/dataplane/health` | dataplane health |
| GET | `/api/dataplane/ipcache` | guest IP → identity |
| POST | `/api/dataplane/refresh-dns` | re-resolve FQDNs |

### Service Fabric v6 (BPF schema 4 — Maglev VIP LB)

Orthogonal Maglev VIP plane (program generation 6 on FluxVM). Full contract:
[ebpf-service-fabric.md](../../ebpf-service-fabric.md).

| Method | Path | Role |
|--------|------|------|
| GET/POST | `/api/dataplane/services` | list / upsert Maglev service |
| GET/DELETE | `/api/dataplane/services/{name}` | get / delete |
| GET | `/api/dataplane/services/status` | host `schema_version`, interfaces, XDP |
| GET | `/api/dataplane/services/stats` | counters |
| GET | `/api/dataplane/services/health` | backend health report |
| POST | `/api/dataplane/services/health/reconcile` | run TCP probes |
| POST | `/api/dataplane/services/conntrack/gc` | expire affinity / NAT |
| GET | `/api/dataplane/services/advertisements` | VIP advertise snapshot |
| GET | `/api/dataplane/services/flows` | FluxScope service flows |
| POST | `/api/dataplane/services/telemetry/export` | OTLP/HTTP JSON export |
| GET | `/api/dataplane/services/{name}/conntrack/delta` | HA delta export |
| POST | `/api/dataplane/services/{name}/conntrack/delta/import` | apply HA delta batch |
| POST | `/api/dataplane/services/{name}/conntrack/delta/ack` | advance source watermark |
| GET/POST | `/api/dataplane/services/policies` | list / upsert identity+L7 policy |
| GET/DELETE | `/api/dataplane/services/{name}/policy` | get / delete policy |
| GET | `/api/dataplane/services/{name}/l7/envoy` | Envoy redirect contract |

Requires FluxVM `[sandbox.dataplane.service] north_south_interfaces` and service
`exposure` north-south/both; north-south NAT needs `snat_address`. Maglev VIP DNAT
on the VM edge stops the TC chain (`TC_ACT_OK`) so backend ports need not appear in
per-VM `allow_ports`.

```bash
curl -sk https://127.0.0.1:9095/api/dataplane/health \
  -H "Authorization: Bearer $TOKEN" | jq
curl -sk https://127.0.0.1:9095/api/vms/NAME/dataplane/status \
  -H "Authorization: Bearer $TOKEN" | jq
curl -sk https://127.0.0.1:9095/api/dataplane/services/status \
  -H "Authorization: Bearer $TOKEN" | jq
fabricctl dataplane service list
fabricctl dataplane service apply --file docs/examples/service-fabric-v3/ha-draining-service.json
fabricctl dataplane service delta payments --after-seq 0
fabricctl dataplane service flows
```

Tutorials: [09-edge-dataplane.md](../../tutorials/09-edge-dataplane.md).

---

## Networking

### Bridges

#### GET /api/networkd/bridges

List all bridge configurations.

**Auth level:** Viewer+

**curl example:**

```bash
curl -s http://localhost:3000/api/networkd/bridges \
  -H "Authorization: Bearer $TOKEN" | jq
```

#### POST /api/networkd/bridges

Create a new bridge interface via direct netlink calls (no systemd-networkd dependency).
Apply is **idempotent**: if the bridge already exists, settings are updated in place
(`ensure_bridge`) instead of failing.

**Auth level:** User+

**Request body:**

```json
{
  "name": "br0",
  "stp": true,
  "forward_delay_sec": 15,
  "hello_time_sec": 2,
  "max_age_sec": 20,
  "vlan_filtering": false,
  "mtu": 1500,
  "mac_address": null,
  "addresses": ["192.168.1.1/24"],
  "gateway": "192.168.1.254",
  "dns": ["8.8.8.8"],
  "dhcp": false
}
```

**Response (201):** Bridge configuration object

**curl example:**

```bash
curl -s -X POST http://localhost:3000/api/networkd/bridges \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "br0",
    "stp": true,
    "addresses": ["192.168.1.1/24"],
    "gateway": "192.168.1.254",
    "dhcp": false
  }' | jq
```

#### DELETE /api/networkd/bridges/:id

Delete a bridge configuration.

**Auth level:** Admin only

---

### VLANs

#### GET /api/networkd/vlans

List all VLAN configurations.

**Auth level:** Viewer+

#### POST /api/networkd/vlans

Create a VLAN interface.

**Auth level:** User+

**Request body:**

```json
{
  "name": "vlan100",
  "id": 100,
  "parent": "eth0",
  "addresses": ["10.0.100.1/24"]
}
```

#### DELETE /api/networkd/vlans/:id

Delete a VLAN.

**Auth level:** Admin only

---

### Bonds

#### GET /api/networkd/bonds

List all bond configurations.

**Auth level:** Viewer+

#### POST /api/networkd/bonds

Create a bond interface.

**Auth level:** User+

**Request body:**

```json
{
  "name": "bond0",
  "mode": "802.3ad",
  "members": ["eth0", "eth1"],
  "mtu": 9000,
  "addresses": ["10.0.0.1/24"]
}
```

#### DELETE /api/networkd/bonds/:id

Delete a bond.

**Auth level:** Admin only

---

### TAP Devices

#### GET /api/networkd/taps

List TAP device configurations.

**Auth level:** Viewer+

#### POST /api/networkd/taps

Create a TAP device.

**Auth level:** User+

**Request body:**

```json
{
  "name": "tap0",
  "bridge": "br0",
  "mtu": 1500
}
```

#### DELETE /api/networkd/taps/:id

Delete a TAP device.

**Auth level:** Admin only

---

### MACVTAP Devices

#### GET /api/networkd/macvtaps

List MACVTAP configurations.

**Auth level:** Viewer+

#### POST /api/networkd/macvtaps

Create a MACVTAP device for direct host NIC passthrough to VMs.

**Auth level:** User+

**Request body:**

```json
{
  "name": "macvtap0",
  "parent": "eth0",
  "mode": "bridge"
}
```

#### DELETE /api/networkd/macvtaps/:id

Delete a MACVTAP device.

**Auth level:** Admin only

---

### Port Forwarding

#### GET /api/networkd/port-forwards

List port forwarding rules (implemented via nftables).

**Auth level:** Viewer+

#### POST /api/networkd/port-forwards

Create a port forwarding rule.

**Auth level:** User+

**Request body:**

```json
{
  "name": "ssh-to-vm",
  "protocol": "tcp",
  "host_port": 2222,
  "dest_ip": "192.168.1.100",
  "dest_port": 22
}
```

#### DELETE /api/networkd/port-forwards/:id

Delete a port forwarding rule.

**Auth level:** Admin only

---

### VXLAN

#### GET /api/networkd/vxlans

List VXLAN tunnel configurations.

**Auth level:** Viewer+

#### POST /api/networkd/vxlans

Create a VXLAN overlay tunnel.

**Auth level:** User+

**Request body:**

```json
{
  "name": "vxlan42",
  "vni": 42,
  "remote": "10.0.0.2",
  "port": 4789,
  "dev": "eth0"
}
```

#### DELETE /api/networkd/vxlans/:id

Delete a VXLAN tunnel.

**Auth level:** Admin only

---

### SR-IOV

#### GET /api/networkd/sriov

List SR-IOV virtual function configurations.

**Auth level:** Viewer+

#### POST /api/networkd/sriov

Configure SR-IOV virtual functions on a physical NIC.

**Auth level:** Admin only

**Request body:**

```json
{
  "name": "sriov-vf0",
  "pf_name": "eth0",
  "num_vfs": 4,
  "vf_configs": []
}
```

#### DELETE /api/networkd/sriov/:id

Remove SR-IOV configuration.

**Auth level:** Admin only

---

### Link Files

#### POST /api/networkd/link-files

Set interface naming/property overrides (applied directly via netlink, not written as systemd-networkd `.link` files).

**Auth level:** Admin only

---

### Network Files

#### POST /api/networkd/network-files

Set address/routing configuration for an interface (applied directly via netlink, not written as systemd-networkd `.network` files).

**Auth level:** Admin only

---

## Storage

### GET /api/storage/pools

List all storage pools.

**Auth level:** Viewer+

**Response (200):** Array of storage pool objects (local, NFS, LVM, ZFS, Ceph)

**curl example:**

```bash
curl -s http://localhost:3000/api/storage/pools \
  -H "Authorization: Bearer $TOKEN" | jq
```

---

### POST /api/storage/pools/local

Create a local directory-based storage pool. Creates the pool state directory under
`{storage.path}/storage` if missing; pool `path` itself must be writable on the host
(e2e uses `/var/lib/zyvor-fabricd/e2e-pool`, not `/tmp`).

**Auth level:** Admin only

**Request body:**

```json
{
  "name": "fast-storage",
  "path": "/var/lib/zyvor-fabricd/images",
  "auto_start": true
}
```

**curl example:**

```bash
curl -s -X POST http://localhost:3000/api/storage/pools/local \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name":"fast-storage","path":"/var/lib/zyvor-fabricd/images","auto_start":true}' | jq
```

---

### POST /api/storage/pools/nfs

Create an NFS-backed storage pool.

**Auth level:** Admin only

**Request body:**

```json
{
  "name": "nfs-pool",
  "config": {
    "server": "10.0.0.5",
    "export": "/exports/vms",
    "version": "4.2",
    "mount_options": "hard,intr"
  }
}
```

---

### POST /api/storage/pools/lvm

Create an LVM-backed storage pool.

**Auth level:** Admin only

**Request body:**

```json
{
  "name": "lvm-pool",
  "volume_group": "vg_vms",
  "auto_start": true
}
```

---

### POST /api/storage/pools/lvm-thin

Create an LVM thin-provisioned storage pool.

**Auth level:** Admin only

**Request body:**

```json
{
  "name": "lvm-thin-pool",
  "volume_group": "vg_vms",
  "thin_pool": "tp_vms",
  "auto_start": true
}
```

---

### POST /api/storage/pools/zfs

Create a ZFS-backed storage pool.

**Auth level:** Admin only

**Request body:**

```json
{
  "name": "zfs-pool",
  "zpool": "rpool",
  "dataset": "vms",
  "auto_start": true
}
```

---

### POST /api/storage/pools/ceph

Create a Ceph RBD storage pool.

**Auth level:** Admin only

**Request body:**

```json
{
  "name": "ceph-pool",
  "pool_name": "vm-images",
  "monitors": ["10.0.0.10:6789"],
  "user": "admin",
  "keyring": "/etc/ceph/ceph.client.admin.keyring"
}
```

---

### GET /api/storage/pools/:name

Get details of a specific storage pool.

**Auth level:** Viewer+

---

### DELETE /api/storage/pools/:name

Delete a storage pool.

**Auth level:** Admin only

---

## Machines API (removed)

The `/api/machines` surface (machinectl / systemd-machined) has been removed. Use FluxVM-backed Virtual Machines: `/api/vms` and `/app/vms`.

---

## Events

### GET /api/events/stream

Subscribe to real-time VM events via Server-Sent Events (SSE).

**Auth level:** Viewer+

**Event format:**

```
event: vm.started
id: 550e8400-e29b-41d4-a716-446655440000
data: {"id":"550e8400...","event_type":"started","vm_name":"my-vm","detail":null,"timestamp":"2026-04-12T10:00:00Z"}
```

**Event types:** `created`, `started`, `stopped`, `paused`, `resumed`, `deleted`, `cloned`, `migrated`, `snapshot_created`, `snapshot_reverted`, `cpu_hotplug`, `memory_hotplug`, `disk_attached`, `disk_detached`, `error`, `auto_healed`

The connection sends periodic keep-alive comments to prevent timeouts. If a client falls behind, a comment is sent indicating the number of missed events.

**curl example:**

```bash
curl -N http://localhost:3000/api/events/stream \
  -H "Authorization: Bearer $TOKEN"
```

---

### GET /api/events

List recent events (up to 100, sorted by timestamp descending).

**Auth level:** Viewer+

**Response (200):**

```json
[
  {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "event_type": "started",
    "vm_name": "my-vm",
    "detail": null,
    "timestamp": "2026-04-12T10:00:00Z"
  }
]
```

**curl example:**

```bash
curl -s http://localhost:3000/api/events \
  -H "Authorization: Bearer $TOKEN" | jq
```

---

## System

Unauthenticated probes (no JWT):

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/health` | Liveness |
| GET | `/readyz` | Readiness — local store + FluxVM `/readyz` (HTTP 503 if not ready) |

```bash
curl -sk https://127.0.0.1:9095/readyz | jq .
```

### GET /api/system/cpu-topology

Get CPU topology of the host (sockets, cores, threads, frequency).

**Auth level:** Viewer+

**curl example:**

```bash
curl -s http://localhost:3000/api/system/cpu-topology \
  -H "Authorization: Bearer $TOKEN" | jq
```

---

### GET /api/system/numa-topology

Get NUMA topology (nodes, CPUs per node, memory per node).

**Auth level:** Viewer+

**curl example:**

```bash
curl -s http://localhost:3000/api/system/numa-topology \
  -H "Authorization: Bearer $TOKEN" | jq
```

---

### GET /api/system/numa/placement

Recommend NUMA placement for a VM based on resource requirements.

**Auth level:** Viewer+

**Query parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `memory_mb` | int | Required memory in MB |
| `cpus` | int | Required CPU count |

**curl example:**

```bash
curl -s "http://localhost:3000/api/system/numa/placement?memory_mb=4096&cpus=4" \
  -H "Authorization: Bearer $TOKEN" | jq
```

---

### POST /api/system/vms/:name/cpu-pinning

Set CPU pinning for a VM.

**Auth level:** User+

**Request body:**

```json
{
  "pinning": {
    "type": "NumaNode",
    "value": 0
  }
}
```

Pinning types: `Auto`, `NumaNode { value }`, `Socket { value }`, `Explicit { value: [{vcpu_id, physical_cpu}] }`

**curl example:**

```bash
curl -s -X POST http://localhost:3000/api/system/vms/my-vm/cpu-pinning \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"pinning": {"type": "Auto"}}' | jq
```

---

### POST /api/system/vms/:name/memory-limit

Set memory cgroup limits for a VM.

**Auth level:** User+

**Request body:**

```json
{
  "limit_bytes": 4294967296,
  "swap_limit_bytes": 2147483648
}
```

---

### POST /api/system/vms/:name/memory-ballooning

Enable or disable memory ballooning.

**Auth level:** User+

**Request body:**

```json
{
  "enabled": true,
  "target_mb": 2048
}
```

---

### POST /api/system/hugepages/allocate

Allocate hugepages on the host.

**Auth level:** Admin only

**Request body:**

```json
{
  "size": "2MB",
  "count": 512
}
```

---

### GET /api/system/hugepages

Get current hugepage allocation status.

**Auth level:** Viewer+

---

### GET /api/system/resource-stats

Get host resource statistics (CPU usage, memory usage, disk I/O).

**Auth level:** Viewer+

**curl example:**

```bash
curl -s http://localhost:3000/api/system/resource-stats \
  -H "Authorization: Bearer $TOKEN" | jq
```

---

## Cloud-init

### POST /api/vms/:name/cloud-init

Generate a cloud-init ISO for a VM. The ISO is saved to `/var/lib/zyvor-fabricd/cloud-init/` and can be attached to the VM at start time.

**Auth level:** User+

**Request body:**

```json
{
  "hostname": "my-vm",
  "user": "admin",
  "ssh_authorized_keys": ["ssh-ed25519 AAAA..."],
  "packages": ["vim", "curl"],
  "runcmd": ["systemctl enable nginx", "systemctl start nginx"],
  "write_files": [
    {
      "path": "/etc/motd",
      "content": "Welcome to my-vm"
    }
  ]
}
```

**Response (200):**

```json
{
  "status": "created",
  "iso_path": "/var/lib/zyvor-fabricd/cloud-init/my-vm.iso"
}
```

**curl example:**

```bash
curl -s -X POST http://localhost:3000/api/vms/my-vm/cloud-init \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "hostname": "my-vm",
    "user": "admin",
    "ssh_authorized_keys": ["ssh-ed25519 AAAA..."]
  }' | jq
```

---

## Notifications

### Channels

#### GET /api/notifications/channels

List all notification channels.

**Auth level:** Viewer+

**Response (200):**

```json
[
  {
    "id": "550e8400...",
    "name": "ops-slack",
    "type": "slack",
    "config": {"webhook_url": "https://hooks.slack.com/..."},
    "enabled": true,
    "created": "2026-04-10T14:30:00Z",
    "last_test": null
  }
]
```

**curl example:**

```bash
curl -s http://localhost:3000/api/notifications/channels \
  -H "Authorization: Bearer $TOKEN" | jq
```

#### POST /api/notifications/channels

Create a notification channel.

**Auth level:** Admin only

**Request body:**

```json
{
  "name": "ops-slack",
  "type": "slack",
  "config": {
    "webhook_url": "https://hooks.slack.com/services/T.../B.../..."
  },
  "enabled": true
}
```

Channel types: `email`, `slack`, `webhook`, `teams`.

Type-specific config fields:

| Type | Config Fields |
|------|--------------|
| `email` | `smtp_server`, `from`, `to`, `username`, `password` |
| `slack` | `webhook_url`, `channel` (optional) |
| `webhook` | `url`, `headers` (optional), `secret` (optional) |
| `teams` | `webhook_url` |

**curl example:**

```bash
curl -s -X POST http://localhost:3000/api/notifications/channels \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "ops-slack",
    "type": "slack",
    "config": {"webhook_url": "https://hooks.slack.com/services/..."},
    "enabled": true
  }' | jq
```

#### PUT /api/notifications/channels/:id

Update a notification channel.

**Auth level:** Admin only

**Request body (partial update):**

```json
{
  "name": "updated-name",
  "config": {"webhook_url": "https://new-url.example.com"},
  "enabled": false
}
```

#### DELETE /api/notifications/channels/:id

Delete a notification channel.

**Auth level:** Admin only

#### POST /api/notifications/channels/:id/test

Send a test notification through a channel to verify configuration.

**Auth level:** Admin only

---

### Rules

#### GET /api/notifications/rules

List all notification rules.

**Auth level:** Viewer+

**Response (200):**

```json
[
  {
    "id": "...",
    "name": "critical-alerts",
    "description": "Alert on VM failures",
    "event_types": ["error", "stopped"],
    "severity_levels": ["critical", "warning"],
    "channels": ["ops-slack-id"],
    "vm_tags": ["production"],
    "enabled": true,
    "triggered_count": 42,
    "last_triggered": "2026-04-12T09:00:00Z"
  }
]
```

#### POST /api/notifications/rules

Create a notification rule.

**Auth level:** Admin only

**Request body:**

```json
{
  "name": "critical-alerts",
  "description": "Alert on VM failures",
  "event_types": ["error", "stopped"],
  "severity_levels": ["critical", "warning"],
  "channels": ["channel-id-1"],
  "vm_tags": ["production"],
  "enabled": true
}
```

Severity levels: `info`, `warning`, `critical`.

**curl example:**

```bash
curl -s -X POST http://localhost:3000/api/notifications/rules \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "critical-alerts",
    "event_types": ["error"],
    "severity_levels": ["critical"],
    "channels": ["channel-id"],
    "enabled": true
  }' | jq
```

#### PUT /api/notifications/rules/:id

Update a notification rule.

**Auth level:** Admin only

#### DELETE /api/notifications/rules/:id

Delete a notification rule.

**Auth level:** Admin only

---

### History

#### GET /api/notifications/history

List notification delivery history.

**Auth level:** Viewer+

---

### Webhook Retry

#### GET /api/notifications/webhooks/deliveries

List webhook delivery attempts with status and retry information.

**Auth level:** Viewer+

**Response (200):** Array of delivery summaries with fields: `id`, `channel_id`, `attempt`, `max_attempts` (max 10), `status`, `response_code`, `error`, `next_retry`.

Retry policy: exponential backoff, maximum 10 retry attempts per delivery, stored payload truncated to 4 KB.

---

## WebSocket Console

### WS /api/vms/:name/console

Open an interactive terminal session with a running VM.

**Authentication:** Pass JWT token as query parameter: `?token=<jwt>`

**Permission:** User+ (write permission required)

**Connection limits:** Maximum 50 concurrent WebSocket connections. Returns `503 Service Unavailable` when the limit is reached.

**Message format:** Binary messages for console I/O (stdin/stdout). Maximum message size: 64 KB.

**Idle timeout:** Connections idle for more than 5 minutes are automatically closed.

**Example (websocat):**

```bash
websocat "ws://localhost:3000/api/vms/my-vm/console?token=$TOKEN"
```

**Example (JavaScript):**

```javascript
const ws = new WebSocket(`ws://localhost:3000/api/vms/my-vm/console?token=${token}`);
ws.binaryType = 'arraybuffer';

ws.onmessage = (event) => {
  const text = new TextDecoder().decode(event.data);
  terminal.write(text);
};

terminal.onData((data) => {
  ws.send(new TextEncoder().encode(data));
});
```

See the [WebSocket Reference](../../reference/api/websocket.md) for full protocol details.

---

## 2FA Authentication

Two-factor authentication using TOTP (Time-based One-Time Password). Once enabled, users must provide a TOTP code along with their credentials at login.

### POST /api/auth/2fa/setup

Set up TOTP 2FA for the current user. Returns a TOTP secret and provisioning URI for use with authenticator apps (Google Authenticator, Authy, etc.).

**Auth level:** Admin only

**Response (200):**

```json
{
  "secret": "JBSWY3DPEHPK3PXP",
  "provisioning_uri": "otpauth://totp/zyvor-fabricd:admin?secret=JBSWY3DPEHPK3PXP&issuer=zyvor-fabricd",
  "qr_code": "data:image/png;base64,..."
}
```

**curl example:**

```bash
curl -s -X POST http://localhost:3000/api/auth/2fa/setup \
  -H "Authorization: Bearer $TOKEN" | jq
```

---

### POST /api/auth/2fa/verify

Verify a TOTP code to finalize 2FA setup. This must be called after `/2fa/setup` to confirm the user has correctly configured their authenticator app.

**Auth level:** Admin only

**Request body:**

```json
{
  "code": "123456"
}
```

**Response (200):**

```json
{
  "status": "2fa_enabled",
  "recovery_codes": ["a1b2c3d4", "e5f6g7h8", "i9j0k1l2"]
}
```

**curl example:**

```bash
curl -s -X POST http://localhost:3000/api/auth/2fa/verify \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"code": "123456"}' | jq
```

---

### POST /api/auth/2fa/disable

Disable 2FA for the current user. Requires a valid TOTP code to confirm the action.

**Auth level:** Admin only

**Request body:**

```json
{
  "code": "654321"
}
```

**Response (200):**

```json
{
  "status": "2fa_disabled"
}
```

**curl example:**

```bash
curl -s -X POST http://localhost:3000/api/auth/2fa/disable \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"code": "654321"}' | jq
```

---

## Secrets Management

Securely store and manage sensitive values (API keys, database credentials, certificates) for use by VMs and system integrations. Secret values are never returned in API responses -- only metadata is exposed.

### GET /api/secrets

List all secrets with metadata. Values are redacted.

**Auth level:** Admin only

**Response (200):**

```json
[
  {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "name": "db-password",
    "description": "Production database password",
    "created": "2026-04-10T14:30:00Z",
    "updated": "2026-04-12T09:15:00Z"
  }
]
```

**curl example:**

```bash
curl -s http://localhost:3000/api/secrets \
  -H "Authorization: Bearer $TOKEN" | jq
```

---

### POST /api/secrets

Create a new secret.

**Auth level:** Admin only

**Request body:**

```json
{
  "name": "db-password",
  "value": "s3cret-p@ssword",
  "description": "Production database password"
}
```

**Response (201):**

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "name": "db-password",
  "description": "Production database password",
  "created": "2026-04-12T10:00:00Z"
}
```

**curl example:**

```bash
curl -s -X POST http://localhost:3000/api/secrets \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "db-password",
    "value": "s3cret-p@ssword",
    "description": "Production database password"
  }' | jq
```

---

### GET /api/secrets/:id

Get secret metadata. The value is redacted.

**Auth level:** Admin only

**Response (200):**

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "name": "db-password",
  "description": "Production database password",
  "created": "2026-04-10T14:30:00Z",
  "updated": "2026-04-12T09:15:00Z"
}
```

**curl example:**

```bash
curl -s http://localhost:3000/api/secrets/550e8400-e29b-41d4-a716-446655440000 \
  -H "Authorization: Bearer $TOKEN" | jq
```

---

### DELETE /api/secrets/:id

Delete a secret permanently.

**Auth level:** Admin only

**Response (204):** No content

**curl example:**

```bash
curl -s -X DELETE http://localhost:3000/api/secrets/550e8400-e29b-41d4-a716-446655440000 \
  -H "Authorization: Bearer $TOKEN"
```

---

## Log Aggregation

Query logs from individual VMs or from the host system. Host system logs (`/api/logs`) are always retrieved via `journalctl`. Per-VM logs (`/api/vms/:name/logs`) come from FluxVM's captured console output, drained through a bounded-wait read — a one-shot fetch, not a live tail.

### GET /api/vms/:name/logs

Get recent logs for a specific VM.

**Auth level:** Viewer+

**Query parameters:**

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `lines`   | int  | 100     | Number of log lines to return (max 10000) |
| `priority`| int  | null    | Syslog priority filter (0=emerg, 3=err, 4=warn, 6=info, 7=debug) |
| `grep`    | string | null  | Pattern to filter log messages |

**Response (200):**

```json
{
  "vm_name": "web-server",
  "lines": [
    {
      "timestamp": "2026-04-12T10:00:00Z",
      "priority": 6,
      "unit": "nginx.service",
      "message": "Started The nginx HTTP and reverse proxy server."
    }
  ],
  "total": 1
}
```

**curl example:**

```bash
# Get last 50 log lines
curl -s "http://localhost:3000/api/vms/web-server/logs?lines=50" \
  -H "Authorization: Bearer $TOKEN" | jq

# Filter by priority (errors and above) and pattern
curl -s "http://localhost:3000/api/vms/web-server/logs?lines=100&priority=3&grep=error" \
  -H "Authorization: Bearer $TOKEN" | jq
```

---

### GET /api/logs

Get system-wide journal logs from the host.

**Auth level:** Viewer+

**Query parameters:**

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `lines`   | int  | 100     | Number of log lines to return (max 10000) |
| `priority`| int  | null    | Syslog priority filter |
| `grep`    | string | null  | Pattern to filter log messages |

**Response (200):**

```json
{
  "lines": [
    {
      "timestamp": "2026-04-12T10:00:00Z",
      "priority": 6,
      "unit": "zyvor-fabricd.service",
      "message": "Listening on 0.0.0.0:3000"
    }
  ],
  "total": 1
}
```

**curl example:**

```bash
# Get system logs
curl -s "http://localhost:3000/api/logs?lines=200" \
  -H "Authorization: Bearer $TOKEN" | jq

# Filter for kernel messages with priority warning or higher
curl -s "http://localhost:3000/api/logs?priority=4&grep=kernel" \
  -H "Authorization: Bearer $TOKEN" | jq
```

---

## VM Export

Export a VM to OVA (Open Virtual Appliance) format for distribution or migration to other hypervisors.

### POST /api/vms/:name/export

Export a VM to OVA format. The VM should be stopped before exporting for consistency.

**Auth level:** Admin only

**Request body:**

```json
{
  "disk_path": "/var/lib/zyvor-fabricd/images/my-vm.qcow2",
  "output_dir": "/var/lib/zyvor-fabricd/exports"
}
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `disk_path` | string | null | Path to the disk image (auto-detected if omitted) |
| `output_dir` | string | `/var/lib/zyvor-fabricd/exports` | Directory for the exported OVA file |

**Response (200):**

```json
{
  "status": "exported",
  "ova_path": "/var/lib/zyvor-fabricd/exports/my-vm.ova",
  "size_bytes": 2147483648
}
```

**curl example:**

```bash
# Export with defaults (auto-detect disk, default output directory)
curl -s -X POST http://localhost:3000/api/vms/my-vm/export \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{}' | jq

# Export with explicit paths
curl -s -X POST http://localhost:3000/api/vms/my-vm/export \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "disk_path": "/var/lib/zyvor-fabricd/images/my-vm.qcow2",
    "output_dir": "/tmp/exports"
  }' | jq
```

---

## Compliance Scanning

Scan VMs against security compliance profiles (e.g., CIS benchmarks) and retrieve scan results.

### GET /api/compliance/profiles

List available compliance profiles.

**Auth level:** Viewer+

**Response (200):**

```json
[
  {
    "id": "cis-level1",
    "name": "CIS Level 1 Baseline",
    "description": "Center for Internet Security Level 1 benchmark",
    "check_count": 85
  }
]
```

**curl example:**

```bash
curl -s http://localhost:3000/api/compliance/profiles \
  -H "Authorization: Bearer $TOKEN" | jq
```

---

### POST /api/compliance/scan/:vm_name

Run a compliance scan against a VM using a specified profile.

**Auth level:** Admin only

**Request body:**

```json
{
  "profile_id": "cis-level1"
}
```

**Response (202):**

```json
{
  "scan_id": "550e8400-e29b-41d4-a716-446655440000",
  "vm_name": "web-server",
  "profile_id": "cis-level1",
  "status": "running",
  "started": "2026-04-12T10:00:00Z"
}
```

**curl example:**

```bash
curl -s -X POST http://localhost:3000/api/compliance/scan/web-server \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"profile_id": "cis-level1"}' | jq
```

---

### GET /api/compliance/results

List compliance scan results.

**Auth level:** Viewer+

**Response (200):**

```json
[
  {
    "scan_id": "550e8400-e29b-41d4-a716-446655440000",
    "vm_name": "web-server",
    "profile_id": "cis-level1",
    "status": "completed",
    "passed": 72,
    "failed": 8,
    "skipped": 5,
    "score": 90.0,
    "started": "2026-04-12T10:00:00Z",
    "completed": "2026-04-12T10:05:00Z"
  }
]
```

**curl example:**

```bash
curl -s http://localhost:3000/api/compliance/results \
  -H "Authorization: Bearer $TOKEN" | jq
```

---

## Billing

Track VM resource usage, configure pricing rules, and generate invoices for tenants.

### GET /api/billing/pricing

Get the current pricing rules.

**Auth level:** Viewer+

**Response (200):**

```json
{
  "cpu_per_hour": 0.01,
  "memory_gb_per_hour": 0.005,
  "disk_gb_per_hour": 0.001,
  "network_egress_per_gb": 0.02,
  "currency": "USD"
}
```

**curl example:**

```bash
curl -s http://localhost:3000/api/billing/pricing \
  -H "Authorization: Bearer $TOKEN" | jq
```

---

### PUT /api/billing/pricing

Update the pricing rules.

**Auth level:** Admin only

**Request body:**

```json
{
  "cpu_per_hour": 0.02,
  "memory_gb_per_hour": 0.008,
  "disk_gb_per_hour": 0.002,
  "network_egress_per_gb": 0.03,
  "currency": "USD"
}
```

**Response (200):**

```json
{
  "status": "updated"
}
```

**curl example:**

```bash
curl -s -X PUT http://localhost:3000/api/billing/pricing \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "cpu_per_hour": 0.02,
    "memory_gb_per_hour": 0.008,
    "disk_gb_per_hour": 0.002,
    "network_egress_per_gb": 0.03,
    "currency": "USD"
  }' | jq
```

---

### GET /api/billing/usage

Get usage records for billing. Records are aggregated per VM per billing period.

**Auth level:** Viewer+

**Response (200):**

```json
[
  {
    "vm_name": "web-server",
    "tenant_id": "tenant-alpha",
    "period_start": "2026-04-01T00:00:00Z",
    "period_end": "2026-04-12T00:00:00Z",
    "cpu_hours": 264.5,
    "memory_gb_hours": 529.0,
    "disk_gb_hours": 10580.0,
    "network_egress_gb": 12.3,
    "total_cost": 15.42
  }
]
```

**curl example:**

```bash
curl -s http://localhost:3000/api/billing/usage \
  -H "Authorization: Bearer $TOKEN" | jq
```

---

### POST /api/billing/invoice/:tenant_id

Generate an invoice for a specific tenant covering the current billing period.

**Auth level:** Admin only

**Response (200):**

```json
{
  "invoice_id": "INV-2026-04-001",
  "tenant_id": "tenant-alpha",
  "period_start": "2026-04-01T00:00:00Z",
  "period_end": "2026-04-12T00:00:00Z",
  "line_items": [
    {"description": "web-server CPU (264.5 hours)", "amount": 5.29},
    {"description": "web-server Memory (529.0 GB-hours)", "amount": 4.23}
  ],
  "total": 15.42,
  "currency": "USD"
}
```

**curl example:**

```bash
curl -s -X POST http://localhost:3000/api/billing/invoice/tenant-alpha \
  -H "Authorization: Bearer $TOKEN" | jq
```

---

## iSCSI Storage

Discover, connect, and manage iSCSI storage targets for VM disk backing.

### POST /api/storage/iscsi/discover

Discover iSCSI targets on a portal.

**Auth level:** Admin only

**Request body:**

```json
{
  "portal": "10.0.0.50:3260"
}
```

**Response (200):**

```json
{
  "targets": [
    {
      "target_name": "iqn.2026-04.com.example:storage",
      "portal": "10.0.0.50:3260"
    }
  ]
}
```

**curl example:**

```bash
curl -s -X POST http://localhost:3000/api/storage/iscsi/discover \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"portal": "10.0.0.50:3260"}' | jq
```

---

### POST /api/storage/iscsi/sign-in

Log in to an iSCSI target to establish a session and make the LUN accessible as a block device.

**Auth level:** Admin only

**Request body:**

```json
{
  "target_name": "iqn.2026-04.com.example:storage",
  "portal": "10.0.0.50:3260",
  "username": "iscsi-user",
  "password": "iscsi-pass"
}
```

**Response (200):**

```json
{
  "status": "logged_in",
  "session_id": "1",
  "device_path": "/dev/sda"
}
```

**curl example:**

```bash
curl -s -X POST http://localhost:3000/api/storage/iscsi/sign-in \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "target_name": "iqn.2026-04.com.example:storage",
    "portal": "10.0.0.50:3260",
    "username": "iscsi-user",
    "password": "iscsi-pass"
  }' | jq
```

---

### POST /api/storage/iscsi/logout

Log out from an active iSCSI session.

**Auth level:** Admin only

**Request body:**

```json
{
  "target_name": "iqn.2026-04.com.example:storage",
  "portal": "10.0.0.50:3260"
}
```

**Response (200):**

```json
{
  "status": "logged_out"
}
```

**curl example:**

```bash
curl -s -X POST http://localhost:3000/api/storage/iscsi/logout \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "target_name": "iqn.2026-04.com.example:storage",
    "portal": "10.0.0.50:3260"
  }' | jq
```

---

### GET /api/storage/iscsi/sessions

List all active iSCSI sessions.

**Auth level:** Viewer+

**Response (200):**

```json
[
  {
    "session_id": "1",
    "target_name": "iqn.2026-04.com.example:storage",
    "portal": "10.0.0.50:3260",
    "state": "LOGGED_IN",
    "device_path": "/dev/sda"
  }
]
```

**curl example:**

```bash
curl -s http://localhost:3000/api/storage/iscsi/sessions \
  -H "Authorization: Bearer $TOKEN" | jq
```

---

## USB Devices

### GET /api/system/usb

List USB devices attached to the host. Useful for identifying devices available for passthrough to VMs.

**Auth level:** Viewer+

**Response (200):**

```json
[
  {
    "bus": 1,
    "device": 3,
    "vendor_id": "0x1234",
    "product_id": "0x5678",
    "manufacturer": "Example Corp",
    "product": "USB Widget",
    "serial": "ABC123"
  }
]
```

**curl example:**

```bash
curl -s http://localhost:3000/api/system/usb \
  -H "Authorization: Bearer $TOKEN" | jq
```

---

## DHCP Server

### POST /api/networkd/dhcp

Configure a DHCP server on a bridge interface. This enables automatic IP address assignment for VMs connected to the bridge. Despite the route's name, this runs a directly-managed `dnsmasq` process per bridge — not systemd-networkd's built-in `[DHCPServer]` — so it works whether or not systemd is present on the host. `pool_start`/`pool_end` must be in the same /24; the bridge's own gateway is assumed to be that subnet's `.1`.

**Auth level:** Admin only

**Request body:**

```json
{
  "bridge_name": "br0",
  "pool_start": "192.168.1.100",
  "pool_end": "192.168.1.200"
}
```

**Response (200):**

```json
{
  "status": "configured",
  "bridge": "br0",
  "pool_offset": 100,
  "pool_size": 101
}
```

**curl example:**

```bash
curl -s -X POST http://localhost:3000/api/networkd/dhcp \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "bridge_name": "br0",
    "pool_start": "192.168.1.100",
    "pool_end": "192.168.1.200"
  }' | jq
```
