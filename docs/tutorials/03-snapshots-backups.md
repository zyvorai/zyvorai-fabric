# Tutorial 03: Snapshots & Backups

Protect your VMs with point-in-time snapshots and automated backup policies.
This tutorial covers creating, listing, reverting, and managing snapshots, as
well as full and incremental backups with retention policies.

**Level:** Intermediate
**Time:** 30 minutes
**Prerequisites:** Completed [Tutorial 01](01-first-vm.md), a running VM with a QCOW2 disk image

---

## What You Will Learn

1. Create disk snapshots of a running or stopped VM
2. List and inspect snapshots
3. Revert a VM to a previous snapshot
4. Navigate snapshot trees
5. Create full and incremental backups
6. Configure automated backup policies
7. Restore a VM from backup

---

## Setup

```bash
export FABRIC_HOST="http://localhost:3000"
TOKEN=$(curl -s "$FABRIC_HOST/api/auth/login" \
  -H "Content-Type: application/json" \
  -d '{"username": "admin", "password": "your-password"}' | jq -r '.token')
```

Create a test VM for this tutorial:

```bash
curl -s -X POST "$FABRIC_HOST/api/vms" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "snapshot-demo",
    "image": "fedora-41",
    "cpus": 2,
    "memory": 2048,
    "disk": 20,
    "tags": ["tutorial"]
  }' | jq .
```

---

## Part 1: Snapshots

Snapshots capture point-in-time state inside the guest's qcow2 image.

| `snapshot_type` | Meaning | How it is taken |
|-----------------|---------|-----------------|
| `Disk` (default) | Disk only | **Running/paused:** QMP `blockdev-snapshot-internal-sync`. **Stopped:** `qemu-img snapshot -c`. |
| `Full` | Disk + guest memory | **Running/paused:** QMP `snapshot-save` (can take minutes under load). **Stopped:** `qemu-img snapshot -c` (no memory to capture). |

Prefer `Disk` for routine checkpoints. Use `Full` only when you need process/memory state. If create returns **409**, the QMP monitor is not ready yet — wait a few seconds after start and retry.

### Step 1: Create a Snapshot

Create a snapshot of the VM. The VM can be running or stopped.

```bash
curl -s -X POST "$FABRIC_HOST/api/vms/snapshot-demo/snapshots" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "before-update",
    "description": "Clean state before applying system updates",
    "snapshot_type": "Disk"
  }' | jq .
```

Expected response:

```json
{
  "id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "vm_name": "snapshot-demo",
  "name": "before-update",
  "description": "Clean state before applying system updates",
  "snapshot_type": "Disk",
  "parent_id": null,
  "size_bytes": 0,
  "created": "2026-04-12T12:00:00Z"
}
```

### Snapshot Parameters

| Parameter       | Type   | Description                              |
|----------------|--------|------------------------------------------|
| `name`         | string | Human-readable snapshot name (unique per VM) |
| `description`  | string | Optional description of the snapshot     |
| `snapshot_type`| enum   | `Disk` (disk only, **default**) or `Full` (disk + memory; slower on a live VM) |

### Step 2: Create Additional Snapshots

Simulate a workflow by creating a chain of snapshots:

```bash
# Snapshot after "installing updates"
curl -s -X POST "$FABRIC_HOST/api/vms/snapshot-demo/snapshots" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "after-update",
    "description": "State after applying kernel update"
  }' | jq .

# Snapshot after "deploying application"
curl -s -X POST "$FABRIC_HOST/api/vms/snapshot-demo/snapshots" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "app-deployed",
    "description": "Application v2.1 deployed and running"
  }' | jq .
```

### Step 3: List Snapshots

```bash
curl -s "$FABRIC_HOST/api/vms/snapshot-demo/snapshots" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

Expected response:

```json
[
  {
    "id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
    "vm_name": "snapshot-demo",
    "name": "before-update",
    "description": "Clean state before applying system updates",
    "snapshot_type": "Disk",
    "parent_id": null,
    "size_bytes": 0,
    "created": "2026-04-12T12:00:00Z"
  },
  {
    "id": "b2c3d4e5-f6a7-8901-bcde-f23456789012",
    "vm_name": "snapshot-demo",
    "name": "after-update",
    "description": "State after applying kernel update",
    "snapshot_type": "Disk",
    "parent_id": null,
    "size_bytes": 0,
    "created": "2026-04-12T12:01:00Z"
  },
  {
    "id": "c3d4e5f6-a7b8-9012-cdef-345678901234",
    "vm_name": "snapshot-demo",
    "name": "app-deployed",
    "description": "Application v2.1 deployed and running",
    "snapshot_type": "Disk",
    "parent_id": null,
    "size_bytes": 0,
    "created": "2026-04-12T12:02:00Z"
  }
]
```

### Step 4: Get Snapshot Details

```bash
curl -s "$FABRIC_HOST/api/vms/snapshot-demo/snapshots/a1b2c3d4-e5f6-7890-abcd-ef1234567890" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

### Step 5: Revert to a Snapshot

The VM **must be stopped** before reverting. This restores the disk to the exact
state captured by the snapshot.

```bash
# Stop the VM first
curl -s -X POST "$FABRIC_HOST/api/vms/snapshot-demo/stop" \
  -H "Authorization: Bearer $TOKEN" | jq .

# Revert to the "before-update" snapshot
curl -s -X POST "$FABRIC_HOST/api/vms/snapshot-demo/snapshots/a1b2c3d4-e5f6-7890-abcd-ef1234567890/revert" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

Expected response:

```json
{
  "status": "reverted",
  "snapshot": "before-update"
}
```

Now start the VM again -- it will be in the state it was when the snapshot was
taken:

```bash
curl -s -X POST "$FABRIC_HOST/api/vms/snapshot-demo/start" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

> **Warning:** Reverting destroys all changes made after the snapshot was taken.
> If you need to preserve the current state, create a new snapshot before
> reverting.

### Step 6: View the Snapshot Tree

When snapshots have parent-child relationships, the tree endpoint shows the
hierarchy:

```bash
curl -s "$FABRIC_HOST/api/vms/snapshot-demo/snapshots/tree" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

Expected response:

```json
[
  {
    "snapshot": {
      "id": "a1b2c3d4-...",
      "name": "before-update",
      "parent_id": null
    },
    "children": [
      {
        "snapshot": {
          "id": "b2c3d4e5-...",
          "name": "after-update",
          "parent_id": "a1b2c3d4-..."
        },
        "children": [
          {
            "snapshot": {
              "id": "c3d4e5f6-...",
              "name": "app-deployed",
              "parent_id": "b2c3d4e5-..."
            },
            "children": []
          }
        ]
      }
    ]
  }
]
```

Visualized:

```
before-update
  +-- after-update
        +-- app-deployed
```

### Step 7: Delete a Snapshot

Deleting a snapshot removes it from both the state store and the QCOW2 image.
This requires Admin privileges.

```bash
curl -s -X DELETE "$FABRIC_HOST/api/vms/snapshot-demo/snapshots/c3d4e5f6-a7b8-9012-cdef-345678901234" \
  -H "Authorization: Bearer $TOKEN"

# Returns 204 No Content
```

---

## Part 2: Backups

Backups copy the VM's disk image to external storage. Unlike snapshots (which
live inside the QCOW2 file), backups are independent files that survive disk
corruption.

### Step 8: Create a Full Backup

```bash
curl -s -X POST "$FABRIC_HOST/api/backups" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "vm_name": "snapshot-demo",
    "backup_type": "full",
    "compress": true,
    "retention_days": 30,
    "description": "Weekly full backup"
  }' | jq .
```

Expected response:

```json
{
  "id": "d4e5f6a7-b8c9-0123-defg-456789012345",
  "backup_id": null,
  "vm_name": "snapshot-demo",
  "operation": "backup",
  "status": "queued",
  "progress": 0.0,
  "started_at": null,
  "completed_at": null,
  "error": null
}
```

The backup runs asynchronously. Monitor its progress:

```bash
curl -s "$FABRIC_HOST/api/backups/jobs/d4e5f6a7-b8c9-0123-defg-456789012345" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

Expected response while running:

```json
{
  "id": "d4e5f6a7-b8c9-0123-defg-456789012345",
  "backup_id": null,
  "vm_name": "snapshot-demo",
  "operation": "backup",
  "status": "running",
  "progress": 45.2,
  "started_at": "2026-04-12T12:10:00Z",
  "completed_at": null,
  "error": null
}
```

### Step 9: Create an Incremental Backup

Incremental backups only save data that changed since the last backup, making
them faster and smaller:

```bash
curl -s -X POST "$FABRIC_HOST/api/backups" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "vm_name": "snapshot-demo",
    "backup_type": "incremental",
    "compress": true,
    "retention_days": 7,
    "description": "Daily incremental"
  }' | jq .
```

### Step 10: List Backups

List all backups:

```bash
curl -s "$FABRIC_HOST/api/backups" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

Filter by VM name:

```bash
curl -s "$FABRIC_HOST/api/backups?vm=snapshot-demo" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

Expected response:

```json
[
  {
    "id": "e5f6a7b8-c9d0-1234-efgh-567890123456",
    "vm_name": "snapshot-demo",
    "backup_type": "full",
    "size_bytes": 1073741824,
    "compressed": true,
    "created": "2026-04-12T12:10:30Z",
    "status": "completed",
    "storage_location": "/var/lib/zyvor-fabricd/backups/snapshot-demo/e5f6a7b8.qcow2.zst",
    "retention_days": 30,
    "expires_at": "2026-05-12T12:10:30Z",
    "metadata": null
  }
]
```

### Step 11: Get Backup Details

```bash
curl -s "$FABRIC_HOST/api/backups/e5f6a7b8-c9d0-1234-efgh-567890123456" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

### Step 12: Restore from Backup

Restore a backup to the original VM or a new VM:

```bash
curl -s -X POST "$FABRIC_HOST/api/backups/restore" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "backup_id": "e5f6a7b8-c9d0-1234-efgh-567890123456",
    "target_vm_name": "snapshot-demo-restored",
    "restore_config": true,
    "restore_disks": true,
    "restore_state": false
  }' | jq .
```

### Restore Options

| Parameter        | Type   | Default | Description                          |
|-----------------|--------|---------|--------------------------------------|
| `backup_id`     | string | --      | ID of the backup to restore          |
| `target_vm_name`| string | null    | New VM name (null = overwrite original) |
| `restore_config`| bool   | true    | Restore VM configuration (CPU, memory) |
| `restore_disks` | bool   | true    | Restore disk images                  |
| `restore_state` | bool   | false   | Restore running state (memory)       |

### Step 13: View Backup Statistics

```bash
curl -s "$FABRIC_HOST/api/backups/stats" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

Expected response:

```json
{
  "total_backups": 5,
  "total_size_bytes": 5368709120,
  "by_type": {
    "full": 2,
    "incremental": 3
  },
  "by_vm": {
    "snapshot-demo": 5
  },
  "oldest_backup": "2026-04-01T00:00:00Z",
  "newest_backup": "2026-04-12T12:15:00Z"
}
```

### Step 14: Delete a Backup

Deleting a backup removes both the state-store entry and the backup file from
disk. Requires Admin privileges.

```bash
curl -s -X DELETE "$FABRIC_HOST/api/backups/e5f6a7b8-c9d0-1234-efgh-567890123456" \
  -H "Authorization: Bearer $TOKEN"

# Returns 204 No Content
```

---

## Part 3: Backup Policies

Automate backups with scheduled policies. Policies define which VMs to back up,
how often, and how long to retain the backups.

### Step 15: Create a Backup Policy

```bash
curl -s -X POST "$FABRIC_HOST/api/backups/policies" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "daily-incremental",
    "vm_tags": ["production"],
    "schedule_type": "daily",
    "backup_type": "incremental",
    "retention_days": 7,
    "enabled": true
  }' | jq .
```

Expected response:

```json
{
  "id": "f6a7b8c9-d0e1-2345-fghi-678901234567",
  "name": "daily-incremental",
  "vm_tags": ["production"],
  "schedule_type": "daily",
  "backup_type": "incremental",
  "retention_days": 7,
  "enabled": true,
  "last_run": null,
  "next_run": "2026-04-13T00:00:00Z"
}
```

### Create a Weekly Full Backup Policy

```bash
curl -s -X POST "$FABRIC_HOST/api/backups/policies" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "weekly-full",
    "vm_tags": ["production"],
    "schedule_type": "weekly",
    "backup_type": "full",
    "retention_days": 30,
    "enabled": true
  }' | jq .
```

### Schedule Types

| Type      | Description                        |
|----------|------------------------------------|
| `daily`  | Run once per day at midnight       |
| `weekly` | Run once per week on Sunday        |
| `monthly`| Run once per month on the first    |

### List Backup Policies

```bash
curl -s "$FABRIC_HOST/api/backups/policies" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

---

## Best Practices

### Snapshot Strategy

1. **Before risky changes:** Always snapshot before OS updates, config changes,
   or application deployments.
2. **Name descriptively:** Use names like `pre-kernel-6.8-update` instead of
   `snap1`.
3. **Clean up old snapshots:** Each snapshot consumes space inside the QCOW2
   file. Delete snapshots you no longer need.
4. **Snapshots are not backups:** Snapshots live inside the disk image. If the
   image is corrupted or deleted, all snapshots are lost.

### Backup Strategy

1. **3-2-1 rule:** Merion 3 copies of data, on 2 different media, with 1 offsite.
2. **Combine full + incremental:** Weekly full backups with daily incrementals
   balance storage cost and recovery speed.
3. **Test restores regularly:** A backup you cannot restore is not a backup.
4. **Set retention policies:** Automatic expiration prevents backup storage from
   growing without bound.

---

## Cleanup

```bash
curl -s -X POST "$FABRIC_HOST/api/vms/snapshot-demo/stop" \
  -H "Authorization: Bearer $TOKEN" | jq .

curl -s -X DELETE "$FABRIC_HOST/api/vms/snapshot-demo" \
  -H "Authorization: Bearer $TOKEN"
```

---

## Next Steps

- [Tutorial 04: Advanced VM Configuration](04-advanced-vm-options.md) -- Fine-tune VMs with TPM, SecureBoot, and hotplug
- [Tutorial 06: Security Hardening](06-security-hardening.md) -- Encrypt VM disks and manage backup access
