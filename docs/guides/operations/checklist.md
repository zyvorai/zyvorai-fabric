# Operational Checklist

Step-by-step checklists for deploying, initializing, and maintaining Zyvor Fabric in production environments.

## Table of Contents

- [Pre-Deployment Checklist](#pre-deployment-checklist)
- [Day 1 Operations](#day-1-operations)
- [Day 2 Operations](#day-2-operations)
- [Disaster Recovery Procedures](#disaster-recovery-procedures)

---

## Pre-Deployment Checklist

Complete these items before deploying Zyvor Fabric to production.

### Host Requirements

`zyvor-fabricd` itself has no systemd dependency, and neither does VM lifecycle -- VMs run under [FluxVM](https://github.com/zyvorai/fluxvm), which supervises each VM's QEMU / Cloud Hypervisor / Firecracker / FluxVM hypervisor process directly.

- [ ] **OS version** -- a modern Linux distribution (Fedora 41+, Ubuntu 24.10+, or equivalent)
- [ ] **Kernel** -- Linux 6.x with KVM support (`/dev/kvm` exists and is accessible)
- [ ] **FluxVM** -- `fluxvm serve` running and reachable at the configured `driver.fluxvm_url`
- [ ] **CPU virtualization** -- Enabled in BIOS/UEFI (`grep -c vmx /proc/cpuinfo` or `grep -c svm /proc/cpuinfo`)
- [ ] **Memory** -- Sufficient RAM for host + all planned VMs (2 GB minimum for host overhead)
- [ ] **Disk space** -- Storage pools provisioned with adequate capacity for VM images and backups

### Software Dependencies

- [ ] **mkosi** -- Installed if building images (`mkosi --version`)
- [ ] **qemu-img** -- Installed for disk operations (`qemu-img --version`)
- [ ] **nftables** -- Installed if using port forwarding or firewall rules (`nft --version`)
- [ ] **PAM** -- System PAM configured for user authentication

### Network Configuration

- [ ] **Firewall port** -- Port 3000 (default) open for API access
- [ ] **Bridge interface** -- Host bridge configured if VMs need external network access
- [ ] **DNS** -- Host DNS resolution working for cloud image downloads

### Security

- [ ] **JWT secret** -- Strong random secret configured (at least 32 characters)
- [ ] **TLS termination** -- Reverse proxy (nginx, caddy) with TLS in front of Zyvor Fabric for production use
- [ ] **System users** -- Administrative user accounts created with appropriate group memberships (wheel/sudo for admin role)
- [ ] **File permissions** -- `/var/lib/zyvor-fabricd/` owned by the zyvor-fabricd service user with restricted permissions

---

## Day 1 Operations

Initial setup tasks after deploying Zyvor Fabric.

### 1. Start the Service

```bash
# Enable and start Zyvor Fabric
sudo systemctl enable --now zyvor-fabricd

# Verify it is running
systemctl status zyvor-fabricd

# Check API liveness + readiness (store + FluxVM)
curl -sk https://127.0.0.1:9095/health
curl -sk https://127.0.0.1:9095/readyz | jq .
```

### 2. Authenticate

```bash
# Login with an admin account
TOKEN=$(curl -s -X POST http://localhost:3000/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"YOUR_PASSWORD"}' | jq -r '.token')

# Verify token works
curl -s http://localhost:3000/api/auth/me \
  -H "Authorization: Bearer $TOKEN" | jq
```

### 3. Configure Storage Pools

```bash
# Create a local storage pool
curl -s -X POST http://localhost:3000/api/storage/pools/local \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "default",
    "path": "/var/lib/zyvor-fabricd/images",
    "auto_start": true
  }' | jq

# (Optional) Create an NFS pool for shared storage
curl -s -X POST http://localhost:3000/api/storage/pools/nfs \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "shared",
    "config": {
      "server": "10.0.0.5",
      "export": "/exports/vms",
      "version": "4.2"
    }
  }' | jq

# Verify pools
curl -s http://localhost:3000/api/storage/pools \
  -H "Authorization: Bearer $TOKEN" | jq
```

### 4. Set Up Networking

```bash
# Create a bridge for VM connectivity
curl -s -X POST http://localhost:3000/api/networkd/bridges \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "br0",
    "stp": true,
    "addresses": ["192.168.100.1/24"],
    "dhcp": false
  }' | jq
```

### 5. Build or Download a Base Image

```bash
# Option A: Build with mkosi
curl -s -X POST http://localhost:3000/api/images/build \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "fedora-base",
    "distribution": "fedora",
    "packages": ["vim", "curl", "htop"],
    "autologin": true
  }' | jq

# Option B: Download a cloud image
curl -s -X POST http://localhost:3000/api/images/download \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"distribution":"fedora","version":"41","format":"qcow2"}' | jq
```

### 6. Create and Start a Test VM

```bash
# Create
curl -s -X POST http://localhost:3000/api/vms \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "test-vm",
    "cpus": 2,
    "memory_mb": 2048,
    "disk_gb": 20
  }' | jq

# Start
curl -s -X POST http://localhost:3000/api/vms/test-vm/start \
  -H "Authorization: Bearer $TOKEN" | jq

# Verify running
curl -s http://localhost:3000/api/vms/test-vm \
  -H "Authorization: Bearer $TOKEN" | jq '.state'
```

### 7. Configure Notifications

```bash
# Create a Slack notification channel
curl -s -X POST http://localhost:3000/api/notifications/channels \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "ops-alerts",
    "type": "slack",
    "config": {"webhook_url": "https://hooks.slack.com/services/..."},
    "enabled": true
  }' | jq

# Test the channel
CHANNEL_ID=$(curl -s http://localhost:3000/api/notifications/channels \
  -H "Authorization: Bearer $TOKEN" | jq -r '.[0].id')

curl -s -X POST "http://localhost:3000/api/notifications/channels/$CHANNEL_ID/test" \
  -H "Authorization: Bearer $TOKEN" | jq

# Create an alert rule for VM failures
curl -s -X POST http://localhost:3000/api/notifications/rules \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d "{
    \"name\": \"critical-alerts\",
    \"event_types\": [\"error\", \"stopped\"],
    \"severity_levels\": [\"critical\"],
    \"channels\": [\"$CHANNEL_ID\"],
    \"enabled\": true
  }" | jq
```

### 8. Set Up Backup Policy

```bash
# Create a daily backup policy for production VMs
curl -s -X POST http://localhost:3000/api/backups/policies \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "daily-production",
    "vm_tags": ["production"],
    "schedule_type": "daily",
    "backup_type": "full",
    "retention_days": 30,
    "enabled": true
  }' | jq
```

### 9. Verify Event Streaming

```bash
# Open an SSE connection to confirm events are flowing
# (run in a separate terminal; press Ctrl+C to stop)
curl -N http://localhost:3000/api/events/stream \
  -H "Authorization: Bearer $TOKEN"
```

### Day 1 Validation Checklist

- [ ] zyvor-fabricd service is running and healthy
- [ ] Authentication works with expected role assignments
- [ ] At least one storage pool is configured
- [ ] Network bridge is operational
- [ ] A test VM can be created, started, stopped, and deleted
- [ ] Notification channel is configured and test message received
- [ ] Backup policy is created and enabled
- [ ] Event stream is producing events for VM lifecycle actions

---

## Day 2 Operations

Ongoing operational tasks for a running Zyvor Fabric deployment.

### Monitoring

- [ ] **Liveness** -- Automated probe of `GET /health` at regular intervals (every 30-60 seconds)
- [ ] **Readiness** -- Probe `GET /readyz` (expects `"ok": true`; HTTP 503 when FluxVM or store is not ready)
- [ ] **Event stream** -- At least one consumer subscribed to `GET /api/events/stream` for real-time alerting
- [ ] **Resource stats** -- Periodic collection of `GET /api/system/resource-stats` for capacity planning
- [ ] **VM metrics** -- Dashboard showing per-VM CPU, memory, and I/O metrics from `GET /api/vms/:name/metrics`
- [ ] **Tenant filter** -- Operators can list with `GET /api/vms?tenant=…` where VMs set `tenant` / `labels.tenant`
- [ ] **JWT tenant** -- Tenant-scoped users have `users.tenant` set; claim enforces create/list/get/mutate
- [ ] **FluxVM auth** -- If FluxVM tokens are on, `driver.fluxvm_token` matches an admin bearer
- [ ] **Hubble (optional)** -- `network.hubble_ui_url` only for an external link ([hubble-ui.md](hubble-ui.md))
- [ ] **Notification delivery** -- Monitor `GET /api/notifications/webhooks/deliveries` for failed webhook deliveries

See the [Monitoring Guide](monitoring.md) for detailed setup instructions.

### Backups

- [ ] **Verify policies** -- Check backup policies are running on schedule (`GET /api/backups/policies`)
- [ ] **Check job status** -- Review backup jobs for failures (`GET /api/backups/jobs`)
- [ ] **Test restores** -- Perform a test restore monthly to verify backup integrity
- [ ] **Monitor storage** -- Track backup storage usage (`GET /api/backups/stats`)
- [ ] **Retention cleanup** -- Verify expired backups are being removed automatically

See the [Backup Strategy Guide](backup-strategy.md) for detailed procedures.

### Updates

- [ ] **Zyvor Fabric updates** -- Test new versions in a staging environment before production
- [ ] **FluxVM updates** -- Test new FluxVM versions in staging before rolling out to hosts running production VMs
- [ ] **Image maintenance** -- Rebuild base images monthly to include OS security patches
- [ ] **Certificate rotation** -- Rotate TLS certificates on the reverse proxy before expiration

### Capacity Planning

- [ ] **Host resources** -- Review CPU and memory utilization trends weekly
- [ ] **Storage growth** -- Monitor storage pool fill rates and expand before reaching 80% capacity
- [ ] **VM density** -- Track the ratio of allocated vs. available resources
- [ ] **NUMA awareness** -- Use `GET /api/system/numa/placement` recommendations for new VM placement

### Security

- [ ] **Audit logs** -- Review `GET /api/audit/logs` weekly for unexpected operations
- [ ] **User accounts** -- Remove or disable accounts for departed team members
- [ ] **JWT secret rotation** -- Rotate the JWT secret periodically (requires service restart; existing tokens become invalid)
- [ ] **Failed login monitoring** -- Check for brute-force patterns in login failure events

---

## Disaster Recovery Procedures

### Scenario 1: Zyvor Fabric Service Failure

**Symptoms:** API returns connection refused; systemd reports `zyvor-fabricd` as failed (if running under systemd).

**Recovery:**

```bash
# 1. Check service status and logs (if running under systemd)
systemctl status zyvor-fabricd
journalctl -u zyvor-fabricd --since "30 min ago" --no-pager

# 2. Restart the service
sudo systemctl restart zyvor-fabricd

# 3. Verify health (works regardless of how the daemon is supervised)
curl -sk https://127.0.0.1:9095/health
curl -sk https://127.0.0.1:9095/readyz | jq '{ok, store, fluxvm_ok: .fluxvm.ok}'

# 4. Check VMs are running (VMs persist independently of zyvor-fabricd)
curl -sf http://127.0.0.1:7788/readyz | jq .
curl -s http://127.0.0.1:7788/v1/vms
```

VMs continue running even if `zyvor-fabricd` restarts. The daemon reconstructs state from FluxVM and the state store on startup.

---

### Scenario 2: VM Stuck in Starting/Stopping State

**Symptoms:** VM shows state `starting` or `stopping` indefinitely.

**Recovery:**

```bash
# 1. Check actual VM status via FluxVM
curl -s http://127.0.0.1:7788/v1/vms | jq '.[] | select(.name == "my-vm")'

# 2. If the VM is running but state is stale, force a stop and restart
curl -s -X POST http://localhost:3000/api/vms/my-vm/stop \
  -H "Authorization: Bearer $TOKEN" | jq

# 3. If FluxVM reports no such VM, terminate via the API directly
curl -s -X POST http://localhost:3000/api/vms/my-vm/terminate \
  -H "Authorization: Bearer $TOKEN" | jq

# 4. Wait briefly, then restart
curl -s -X POST http://localhost:3000/api/vms/my-vm/start \
  -H "Authorization: Bearer $TOKEN" | jq
```

---

### Scenario 3: Host Reboot Recovery

**Symptoms:** All VMs stopped after unexpected host reboot.

**Recovery:**

```bash
# 1. Ensure Zyvor Fabric starts automatically
sudo systemctl enable zyvor-fabricd

# 2. List VMs that need manual start
curl -s http://localhost:3000/api/vms \
  -H "Authorization: Bearer $TOKEN" | jq '.items[] | select(.state != "running") | .name'

# 4. Start VMs as needed
for vm in web-server db-server; do
  curl -s -X POST "http://localhost:3000/api/vms/$vm/start" \
    -H "Authorization: Bearer $TOKEN" | jq
done
```

---

### Scenario 4: Storage Pool Failure

**Symptoms:** VM operations fail with disk I/O errors; storage pool reports errors.

**Recovery:**

```bash
# 1. Check storage pool status
curl -s http://localhost:3000/api/storage/pools \
  -H "Authorization: Bearer $TOKEN" | jq

# 2. For NFS pools, verify network connectivity and mount
mount | grep nfs
ping -c 3 nfs-server

# 3. For LVM pools, check volume group status
sudo vgs
sudo lvs

# 4. For ZFS pools, check pool health
sudo zpool status

# 5. If pool is degraded, stop affected VMs before repair
curl -s -X POST http://localhost:3000/api/vms/affected-vm/stop \
  -H "Authorization: Bearer $TOKEN" | jq
```

---

### Scenario 5: Full Restore from Backup

**Symptoms:** VM data is corrupted or lost. A full restore from backup is required.

**Recovery:**

```bash
# 1. List available backups for the VM
curl -s "http://localhost:3000/api/backups?vm=my-vm" \
  -H "Authorization: Bearer $TOKEN" | jq '.[0]'

# 2. Stop the VM if it is running
curl -s -X POST http://localhost:3000/api/vms/my-vm/stop \
  -H "Authorization: Bearer $TOKEN" | jq

# 3. Restore from the most recent backup
BACKUP_ID=$(curl -s "http://localhost:3000/api/backups?vm=my-vm" \
  -H "Authorization: Bearer $TOKEN" | jq -r '.[0].id')

curl -s -X POST http://localhost:3000/api/backups/restore \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d "{
    \"backup_id\": \"$BACKUP_ID\",
    \"restore_config\": true,
    \"restore_disks\": true
  }" | jq

# 4. Start the restored VM
curl -s -X POST http://localhost:3000/api/vms/my-vm/start \
  -H "Authorization: Bearer $TOKEN" | jq

# 5. Verify VM is operational
curl -s http://localhost:3000/api/vms/my-vm \
  -H "Authorization: Bearer $TOKEN" | jq
```

---

### Scenario 6: Network Connectivity Loss

**Symptoms:** VMs cannot reach the external network or each other.

**Recovery:**

```bash
# 1. Verify bridge interface is up (host networking is managed via direct
#    netlink calls, not systemd-networkd -- no separate service to check,
#    and no reload step: changes apply immediately)
ip link show br0
ip addr show br0

# 2. Check nftables rules
sudo nft list ruleset

# 3. Verify network configurations managed by Zyvor Fabric
curl -s http://localhost:3000/api/networkd/bridges \
  -H "Authorization: Bearer $TOKEN" | jq
```

---

### Emergency Contact Information

Maintain a runbook with these details:

- [ ] On-call rotation contact information
- [ ] Notification channel IDs for escalation
- [ ] Backup storage locations and access credentials
- [ ] Host IPMI/BMC access for out-of-band recovery
- [ ] Network diagram showing VM connectivity and bridge topology
