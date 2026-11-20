# Backup Strategy Guide

How to configure, automate, and verify backups for VMs managed by Zyvor Fabric.

## Table of Contents

- [Backup Types](#backup-types)
- [Manual Backups](#manual-backups)
- [Backup Policies and Scheduling](#backup-policies-and-scheduling)
- [Retention Policies](#retention-policies)
- [Restore Procedures](#restore-procedures)
- [Testing Backups](#testing-backups)
- [Storage Considerations](#storage-considerations)

---

## Backup Types

Zyvor Fabric supports two backup types:

### Full Backup

A complete copy of the VM's disk image and configuration.

| Attribute | Value |
|-----------|-------|
| Type identifier | `full` |
| Includes | Disk image + VM configuration |
| Size | Equal to the used portion of the disk (compressed by default) |
| Restore speed | Fastest -- standalone, no dependencies |
| Best for | Weekly or monthly baseline backups |

### Incremental Backup

Captures only the blocks that changed since the last backup (full or incremental).

| Attribute | Value |
|-----------|-------|
| Type identifier | `incremental` |
| Includes | Changed disk blocks + VM configuration delta |
| Size | Significantly smaller than full (typically 5-20%) |
| Restore speed | Slower -- requires the base full backup + all incremental chain |
| Best for | Daily backups between full backups |

### Recommended Strategy

For most production environments, use a combination:

```
Week 1: Full (Sunday) -> Incremental (Mon-Sat)
Week 2: Full (Sunday) -> Incremental (Mon-Sat)
...
```

This balances storage efficiency with restore speed. A full backup is always available within the last 7 days.

---

## Manual Backups

### Creating a Backup

```bash
# Full backup with compression
curl -s -X POST http://localhost:3000/api/backups \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "vm_name": "my-vm",
    "backup_type": "full",
    "compress": true,
    "retention_days": 30,
    "description": "Pre-upgrade backup"
  }' | jq
```

### Listing Backups

```bash
# All backups
curl -s http://localhost:3000/api/backups \
  -H "Authorization: Bearer $TOKEN" | jq

# Backups for a specific VM
curl -s "http://localhost:3000/api/backups?vm=my-vm" \
  -H "Authorization: Bearer $TOKEN" | jq

# Most recent backup for a VM
curl -s "http://localhost:3000/api/backups?vm=my-vm" \
  -H "Authorization: Bearer $TOKEN" | jq '.[0]'
```

### Checking Backup Job Status

Long-running backup operations run asynchronously. Monitor progress via the jobs endpoint:

```bash
curl -s http://localhost:3000/api/backups/jobs \
  -H "Authorization: Bearer $TOKEN" | jq '.[] | {vm_name, operation, status, progress}'
```

Job status values: `queued`, `running`, `completed`, `failed`.

The `progress` field ranges from 0.0 to 1.0.

---

## Backup Policies and Scheduling

Backup policies automate recurring backups based on a schedule. Policies can target VMs by tag, making them easy to manage at scale.

### Creating a Policy

```bash
# Daily full backups for production VMs, retained for 30 days
curl -s -X POST http://localhost:3000/api/backups/policies \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "prod-daily-full",
    "vm_tags": ["production"],
    "schedule_type": "daily",
    "backup_type": "full",
    "retention_days": 30,
    "enabled": true
  }' | jq

# Weekly full backup for all VMs, retained for 90 days
curl -s -X POST http://localhost:3000/api/backups/policies \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "weekly-full-all",
    "vm_tags": null,
    "schedule_type": "weekly",
    "backup_type": "full",
    "retention_days": 90,
    "enabled": true
  }' | jq

# Monthly archive backup retained for a year
curl -s -X POST http://localhost:3000/api/backups/policies \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "monthly-archive",
    "vm_tags": ["production"],
    "schedule_type": "monthly",
    "backup_type": "full",
    "retention_days": 365,
    "enabled": true
  }' | jq
```

### Schedule Types

| Schedule | Frequency | Use Case |
|----------|-----------|----------|
| `daily` | Once per day | Incremental or full for active VMs |
| `weekly` | Once per week | Full baseline backup |
| `monthly` | Once per month | Long-term archive |

### Managing Policies

```bash
# List all policies
curl -s http://localhost:3000/api/backups/policies \
  -H "Authorization: Bearer $TOKEN" | jq

# Check last run and next scheduled run
curl -s http://localhost:3000/api/backups/policies \
  -H "Authorization: Bearer $TOKEN" | jq '.[] | {name, last_run, next_run, enabled}'
```

### Tag-Based Targeting

Policies use `vm_tags` to select which VMs to back up. When creating VMs, assign tags to control which policies apply:

```bash
# Create a VM with a production tag
curl -s -X POST http://localhost:3000/api/vms \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "web-server",
    "cpus": 4,
    "memory_mb": 4096,
    "disk_gb": 40,
    "labels": {"env": "production"}
  }' | jq
```

If `vm_tags` is `null` or omitted, the policy applies to all VMs.

---

## Retention Policies

Each backup has a `retention_days` value that determines how long it is kept. Expired backups are cleaned up automatically.

### Retention Guidelines

| Environment | Full Backup Retention | Incremental Retention |
|-------------|----------------------|----------------------|
| Development | 7 days | 3 days |
| Staging | 14 days | 7 days |
| Production | 30-90 days | 14 days |
| Compliance/Archive | 365+ days | N/A |

### Monitoring Retention

```bash
# View backup expiration dates
curl -s http://localhost:3000/api/backups \
  -H "Authorization: Bearer $TOKEN" | jq '.[] | {id, vm_name, created, expires_at}'

# Check aggregate storage usage
curl -s http://localhost:3000/api/backups/stats \
  -H "Authorization: Bearer $TOKEN" | jq '{total_backups, total_size_bytes}'
```

### Manual Cleanup

To delete a specific backup before its retention period expires:

```bash
curl -s -X DELETE http://localhost:3000/api/backups/<backup-id> \
  -H "Authorization: Bearer $TOKEN"
```

This requires Admin permissions.

---

## Restore Procedures

### Restore In-Place

Restore a backup over the existing VM (overwrites current state):

```bash
# 1. Stop the VM
curl -s -X POST http://localhost:3000/api/vms/my-vm/stop \
  -H "Authorization: Bearer $TOKEN" | jq

# 2. Restore from backup
curl -s -X POST http://localhost:3000/api/backups/restore \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "backup_id": "BACKUP_ID",
    "restore_config": true,
    "restore_disks": true,
    "restore_state": false
  }' | jq

# 3. Start the VM
curl -s -X POST http://localhost:3000/api/vms/my-vm/start \
  -H "Authorization: Bearer $TOKEN" | jq
```

### Restore to a New VM

Create a new VM from a backup, leaving the original untouched:

```bash
curl -s -X POST http://localhost:3000/api/backups/restore \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "backup_id": "BACKUP_ID",
    "target_vm_name": "my-vm-restored",
    "restore_config": true,
    "restore_disks": true,
    "restore_state": false
  }' | jq
```

### Restore Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `backup_id` | string | required | Backup to restore from |
| `target_vm_name` | string | null | New VM name (null = restore in place) |
| `restore_config` | bool | true | Restore VM configuration (CPU, memory, etc.) |
| `restore_disks` | bool | true | Restore disk images |
| `restore_state` | bool | false | Restore memory state if available (full snapshots only) |

### Monitoring Restore Progress

```bash
# Check restore job status
curl -s http://localhost:3000/api/backups/jobs \
  -H "Authorization: Bearer $TOKEN" | jq '.[] | select(.operation == "restore")'
```

---

## Testing Backups

Untested backups are unreliable. Follow these procedures to verify backup integrity.

### Monthly Restore Test

Perform this procedure at least once per month for production VMs:

```bash
# 1. Select the most recent backup
BACKUP_ID=$(curl -s "http://localhost:3000/api/backups?vm=production-vm" \
  -H "Authorization: Bearer $TOKEN" | jq -r '.[0].id')

# 2. Restore to a temporary VM
curl -s -X POST http://localhost:3000/api/backups/restore \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d "{
    \"backup_id\": \"$BACKUP_ID\",
    \"target_vm_name\": \"restore-test-$(date +%Y%m%d)\"
  }" | jq

# 3. Start the restored VM and verify it boots
TEST_VM="restore-test-$(date +%Y%m%d)"
curl -s -X POST "http://localhost:3000/api/vms/$TEST_VM/start" \
  -H "Authorization: Bearer $TOKEN" | jq

# 4. Wait for it to reach running state, then run a health check
sleep 30
curl -s "http://localhost:3000/api/vms/$TEST_VM" \
  -H "Authorization: Bearer $TOKEN" | jq '.state'

# 5. Clean up: stop and delete the test VM
curl -s -X POST "http://localhost:3000/api/vms/$TEST_VM/stop" \
  -H "Authorization: Bearer $TOKEN" | jq
curl -s -X DELETE "http://localhost:3000/api/vms/$TEST_VM" \
  -H "Authorization: Bearer $TOKEN"
```

### Backup Integrity Checklist

- [ ] Backup job completed with status `completed` (not `failed`)
- [ ] Backup `size_bytes` is reasonable (not zero, not suspiciously small)
- [ ] Restored VM boots successfully
- [ ] Application data is present and intact in the restored VM
- [ ] Network connectivity works from the restored VM
- [ ] Restore time is within acceptable RTO (Recovery Time Objective)

---

## Storage Considerations

### Default Backup Location

Backups are stored in `/var/lib/zyvor-fabricd/backups/` by default. This can be overridden by setting the `BACKUP_DIR` environment variable.

### Compression

Backups are compressed by default (`compress: true`). Compression reduces storage usage by 40-70% depending on the workload but increases backup/restore time. Disable compression for VMs with already-compressed data (databases with native compression, media files).

### Storage Capacity Planning

Estimate required storage with this formula:

```
Storage needed = (Number of VMs) x (Average disk size) x (Full backups kept)
               + (Number of VMs) x (Average daily change rate) x (Incremental backups kept)
```

Example for 10 VMs with 40 GB disks, 5% daily change rate:
- 4 weekly full backups: 10 x 40 GB x 4 = 1,600 GB (before compression)
- 28 daily incremental: 10 x 2 GB x 28 = 560 GB
- Total with compression (~50%): approximately 1,080 GB

### Remote Backup Storage

For production environments, consider storing backups on a separate storage system:

- **NFS pool** -- Mount a remote NFS export and set `BACKUP_DIR` to the mount point
- **Object storage** -- Use a scheduled job to sync backups to S3-compatible storage
- **Separate physical disk** -- Use a dedicated disk or RAID array for backup storage to protect against single-disk failures
