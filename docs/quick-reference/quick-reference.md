# Zyvor Fabric Quick Reference

One-page cheat sheet for common operations with Zyvor Fabric.

---

## Authentication

```bash
# Log in and get a JWT token
TOKEN=$(curl -s http://127.0.0.1:9095/api/v1/auth/sign-in \
  -H "Content-Type: application/json" \
  -d '{"username": "admin", "password": "YOUR_PASSWORD"}' | jq -r '.token')

# Check current user
curl -s http://127.0.0.1:9095/api/v1/auth/me \
  -H "Authorization: Bearer $TOKEN" | jq .
```

## OpenStack Compatibility (experimental)

Same port as Fabric. Set `daemon.public_url` / `ZYVOR_FABRICD_PUBLIC_URL` for remote hosts.
Full guide: [openstack-compat.md](../openstack-compat.md) ·
[Tutorial 08](../tutorials/08-openstack-clients.md).

```bash
BASE=https://127.0.0.1:9095
curl -sk -D /tmp/os-hdrs -o /tmp/tok.json -X POST "$BASE/identity/v3/auth/tokens" \
  -H "Content-Type: application/json" \
  -d '{"auth":{"identity":{"methods":["password"],"password":{"user":{"name":"admin","password":"any"}}},"scope":{"project":{"name":"admin"}}}}'
TOKEN=$(awk -F': ' 'tolower($1)=="x-subject-token"{print $2}' /tmp/os-hdrs | tr -d '\r')
curl -sk "$BASE/compute/v2.1/flavors" -H "X-Auth-Token: $TOKEN" | jq .
```

## 2FA Setup and Login

```bash
# Set up 2FA (get TOTP secret)
curl -s -X POST http://127.0.0.1:9095/api/v1/auth/2fa/setup \
  -H "Authorization: Bearer $TOKEN" | jq .

# Verify and activate 2FA
curl -s -X POST http://127.0.0.1:9095/api/v1/auth/2fa/verify \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"totp_code": "123456"}' | jq .

# Log in with 2FA
TOKEN=$(curl -s http://127.0.0.1:9095/api/v1/auth/sign-in \
  -H "Content-Type: application/json" \
  -d '{"username": "admin", "password": "YOUR_PASSWORD", "totp_code": "654321"}' | jq -r '.token')
```

---

## VM Lifecycle

### Create a VM

```bash
curl -s -X POST http://127.0.0.1:9095/api/v1/vms \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "my-vm",
    "image": "fedora-40.raw",
    "cpus": 2,
    "memory": 2048,
    "disk": 20,
    "hostname": "my-vm.local",
    "tags": ["web", "production"]
  }' | jq .
```

### List VMs

```bash
# List all VMs
curl -s http://127.0.0.1:9095/api/v1/vms \
  -H "Authorization: Bearer $TOKEN" | jq .

# With pagination
curl -s "http://127.0.0.1:9095/api/v1/vms?offset=0&limit=50" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

### Get VM Details

```bash
curl -s http://127.0.0.1:9095/api/v1/vms/my-vm \
  -H "Authorization: Bearer $TOKEN" | jq .
```

### Start a VM

```bash
curl -s -X POST http://127.0.0.1:9095/api/v1/vms/my-vm/start \
  -H "Authorization: Bearer $TOKEN" | jq .
```

### Stop a VM

```bash
curl -s -X POST http://127.0.0.1:9095/api/v1/vms/my-vm/stop \
  -H "Authorization: Bearer $TOKEN" | jq .
```

### Restart a VM

```bash
curl -s -X POST http://127.0.0.1:9095/api/v1/vms/my-vm/restart \
  -H "Authorization: Bearer $TOKEN" | jq .
```

### Pause / Resume

```bash
# Pause
curl -s -X POST http://127.0.0.1:9095/api/v1/vms/my-vm/pause \
  -H "Authorization: Bearer $TOKEN" | jq .

# Resume
curl -s -X POST http://127.0.0.1:9095/api/v1/vms/my-vm/resume \
  -H "Authorization: Bearer $TOKEN" | jq .
```

### Clone a VM

```bash
curl -s -X POST http://127.0.0.1:9095/api/v1/vms/my-vm/clone \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name": "my-vm-clone"}' | jq .
```

### Delete a VM

```bash
curl -s -X DELETE http://127.0.0.1:9095/api/v1/vms/my-vm \
  -H "Authorization: Bearer $TOKEN" | jq .
```

### OVA Export

```bash
# Export a VM to OVA format
curl -s -X POST http://127.0.0.1:9095/api/v1/vms/my-vm/export/ova \
  -H "Authorization: Bearer $TOKEN" | jq .

# Download the exported OVA file
curl -s http://127.0.0.1:9095/api/v1/vms/my-vm/export/ova/download \
  -H "Authorization: Bearer $TOKEN" -o my-vm.ova
```

---

## Snapshots

```bash
# Create a snapshot
curl -s -X POST http://127.0.0.1:9095/api/v1/vms/my-vm/snapshots \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"description": "Before upgrade"}' | jq .

# List snapshots
curl -s http://127.0.0.1:9095/api/v1/vms/my-vm/snapshots \
  -H "Authorization: Bearer $TOKEN" | jq .

# Revert to a snapshot
curl -s -X POST http://127.0.0.1:9095/api/v1/vms/my-vm/snapshots/SNAPSHOT_ID/revert \
  -H "Authorization: Bearer $TOKEN" | jq .
```

---

## Images

```bash
# List available images
curl -s http://127.0.0.1:9095/api/v1/images \
  -H "Authorization: Bearer $TOKEN" | jq .

# List cloud images available for download
curl -s http://127.0.0.1:9095/api/v1/images/cloud \
  -H "Authorization: Bearer $TOKEN" | jq .

# Download a cloud image
curl -s -X POST http://127.0.0.1:9095/api/v1/images/cloud/download \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"url": "https://download.fedoraproject.org/pub/fedora/linux/releases/40/Cloud/x86_64/images/Fedora-Cloud-Base-40-1.14.x86_64.raw.xz", "name": "fedora-40"}' | jq .
```

---

## Metrics and Monitoring

```bash
# Get VM metrics
curl -s http://127.0.0.1:9095/api/v1/vms/my-vm/metrics \
  -H "Authorization: Bearer $TOKEN" | jq .

# System performance
curl -s http://127.0.0.1:9095/api/v1/analytics/system \
  -H "Authorization: Bearer $TOKEN" | jq .

# Prometheus metrics (no auth required)
curl -s http://127.0.0.1:9095/metrics

# Network metrics
curl -s http://127.0.0.1:9095/api/v1/network-metrics/my-vm \
  -H "Authorization: Bearer $TOKEN" | jq .
```

---

## Networking

```bash
# List bridges
curl -s http://127.0.0.1:9095/api/v1/networkd/bridges \
  -H "Authorization: Bearer $TOKEN" | jq .

# List network interfaces (via netlink)
curl -s http://127.0.0.1:9095/api/v1/networkd/netlink/interfaces \
  -H "Authorization: Bearer $TOKEN" | jq .

# Create a VLAN
curl -s -X POST http://127.0.0.1:9095/api/v1/networkd/vlans \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name": "vlan100", "vlan_id": 100, "parent": "eth0"}' | jq .

# List firewall profiles
curl -s http://127.0.0.1:9095/api/v1/firewall-profiles \
  -H "Authorization: Bearer $TOKEN" | jq .
```

---

## Backups

```bash
# Create a backup
curl -s -X POST http://127.0.0.1:9095/api/v1/backups \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"vm_name": "my-vm", "description": "Weekly backup"}' | jq .

# List backups
curl -s http://127.0.0.1:9095/api/v1/backups \
  -H "Authorization: Bearer $TOKEN" | jq .

# Restore a backup
curl -s -X POST http://127.0.0.1:9095/api/v1/backups/restore \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"backup_id": "BACKUP_ID"}' | jq .
```

---

## Templates

```bash
# Create a template from a VM
curl -s -X POST http://127.0.0.1:9095/api/v1/templates \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name": "web-server-template", "source_vm": "my-vm"}' | jq .

# Deploy a VM from a template
curl -s -X POST http://127.0.0.1:9095/api/v1/templates/TEMPLATE_ID/deploy \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"vm_name": "web-server-2"}' | jq .
```

---

## System Information

```bash
# CPU topology
curl -s http://127.0.0.1:9095/api/v1/system/cpu/topology \
  -H "Authorization: Bearer $TOKEN" | jq .

# NUMA topology
curl -s http://127.0.0.1:9095/api/v1/system/numa/topology \
  -H "Authorization: Bearer $TOKEN" | jq .

# System memory
curl -s http://127.0.0.1:9095/api/v1/system/memory \
  -H "Authorization: Bearer $TOKEN" | jq .

# Firmware capabilities
curl -s http://127.0.0.1:9095/api/v1/system/firmware/capabilities \
  -H "Authorization: Bearer $TOKEN" | jq .
```

---

## Secrets Management

```bash
# Create a secret
curl -s -X POST http://127.0.0.1:9095/api/v1/secrets \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name": "db-pass", "value": "s3cret", "description": "DB password"}' | jq .

# List secrets
curl -s http://127.0.0.1:9095/api/v1/secrets \
  -H "Authorization: Bearer $TOKEN" | jq .

# Get secret metadata
curl -s http://127.0.0.1:9095/api/v1/secrets/db-pass \
  -H "Authorization: Bearer $TOKEN" | jq .

# Update a secret
curl -s -X PUT http://127.0.0.1:9095/api/v1/secrets/db-pass \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"value": "new-s3cret"}' | jq .

# Delete a secret
curl -s -X DELETE http://127.0.0.1:9095/api/v1/secrets/db-pass \
  -H "Authorization: Bearer $TOKEN" | jq .
```

---

## Compliance Scanning

```bash
# List compliance profiles
curl -s http://127.0.0.1:9095/api/v1/compliance/profiles \
  -H "Authorization: Bearer $TOKEN" | jq .

# Scan a VM
curl -s -X POST http://127.0.0.1:9095/api/v1/compliance/scan \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"vm_name": "my-vm", "profile_id": "cis-level1"}' | jq .

# View scan findings
curl -s http://127.0.0.1:9095/api/v1/compliance/scans/SCAN_ID/findings \
  -H "Authorization: Bearer $TOKEN" | jq .

# Scan history for a VM
curl -s "http://127.0.0.1:9095/api/v1/compliance/scans?vm_name=my-vm" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

---

## Billing and Usage

```bash
# Get current usage summary
curl -s http://127.0.0.1:9095/api/v1/billing/usage \
  -H "Authorization: Bearer $TOKEN" | jq .

# Get usage for a specific VM
curl -s http://127.0.0.1:9095/api/v1/billing/usage/my-vm \
  -H "Authorization: Bearer $TOKEN" | jq .

# List invoices
curl -s http://127.0.0.1:9095/api/v1/billing/invoices \
  -H "Authorization: Bearer $TOKEN" | jq .

# Get a specific invoice
curl -s http://127.0.0.1:9095/api/v1/billing/invoices/INVOICE_ID \
  -H "Authorization: Bearer $TOKEN" | jq .

# Get pricing tiers
curl -s http://127.0.0.1:9095/api/v1/billing/pricing \
  -H "Authorization: Bearer $TOKEN" | jq .
```

---

## Log Aggregation

```bash
# Get logs for a VM
curl -s http://127.0.0.1:9095/api/v1/logs/my-vm \
  -H "Authorization: Bearer $TOKEN" | jq .

# Search logs with a query
curl -s "http://127.0.0.1:9095/api/v1/logs?query=error&vm_name=my-vm&limit=50" \
  -H "Authorization: Bearer $TOKEN" | jq .

# Stream logs (SSE)
curl -s -N http://127.0.0.1:9095/api/v1/logs/my-vm/stream \
  -H "Authorization: Bearer $TOKEN"

# Get log statistics
curl -s http://127.0.0.1:9095/api/v1/logs/stats \
  -H "Authorization: Bearer $TOKEN" | jq .
```

---

## iSCSI Storage

```bash
# Discover iSCSI targets
curl -s -X POST http://127.0.0.1:9095/api/v1/iscsi/discover \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"portal": "192.168.1.100:3260"}' | jq .

# Log in to an iSCSI target
curl -s -X POST http://127.0.0.1:9095/api/v1/iscsi/sign-in \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"portal": "192.168.1.100:3260", "target": "iqn.2026-01.com.example:storage"}' | jq .

# List active iSCSI sessions
curl -s http://127.0.0.1:9095/api/v1/iscsi/sessions \
  -H "Authorization: Bearer $TOKEN" | jq .

# Log out of an iSCSI target
curl -s -X POST http://127.0.0.1:9095/api/v1/iscsi/logout \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"session_id": "SESSION_ID"}' | jq .
```

---

## USB Passthrough

```bash
# List available USB devices on the host
curl -s http://127.0.0.1:9095/api/v1/usb/devices \
  -H "Authorization: Bearer $TOKEN" | jq .

# Attach a USB device to a VM
curl -s -X POST http://127.0.0.1:9095/api/v1/vms/my-vm/usb/attach \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"vendor_id": "0x1234", "product_id": "0x5678"}' | jq .

# List USB devices attached to a VM
curl -s http://127.0.0.1:9095/api/v1/vms/my-vm/usb \
  -H "Authorization: Bearer $TOKEN" | jq .

# Detach a USB device from a VM
curl -s -X DELETE http://127.0.0.1:9095/api/v1/vms/my-vm/usb/DEVICE_ID \
  -H "Authorization: Bearer $TOKEN" | jq .
```

---

## Configuration Quick Reference

### zyvor-fabricd.toml

```toml
[daemon]
listen = "127.0.0.1:9095"       # Bind address
# public_url = "https://fabric.example.com:9095"  # OpenStack catalog / clients
cors_origins = ["http://..."]    # Allowed CORS origins

[storage]
path = "/var/lib/zyvor-fabricd"       # State directory
image_path = "/var/lib/zyvor-fabricd/images"  # VM images

[network]
bridge = "br0"                   # Default bridge
# networkd_config_dir/networkd_file_prefix still exist as config fields but
# are now vestigial -- host networking is applied directly via netlink
# calls, not by writing systemd-networkd config files

[auth]
enabled = true                   # Enable authentication
db_path = "/var/lib/zyvor-fabricd/auth.db"
token_expiration_hours = 24      # JWT lifetime

[controller]
enabled = false                  # Multi-host mode
mode = "standalone"              # or "controller"
```

### Environment Variables

| Variable                  | Purpose                              |
|---------------------------|--------------------------------------|
| `ZYVOR_FABRICD_LOG_LEVEL`        | Log level: trace/debug/info/warn/error |
| `RUST_LOG`                | Fallback log filter                  |
| `ZYVOR_FABRICD_JWT_SECRET`     | JWT signing secret                   |
| `ZYVOR_FABRICD_ADMIN_PASSWORD` | Initial admin password               |

---

## Troubleshooting Quick Reference

### Daemon will not start

```bash
# Check for port conflicts
ss -tlnp | grep 9095

# Check systemd service status (if running under systemd -- optional,
# zyvor-fabricd has no hard systemd dependency)
sudo systemctl status zyvor-fabricd
journalctl -u zyvor-fabricd -n 50

# Check config file syntax
cat /etc/zyvor-fabricd/zyvor-fabricd.toml | toml-lint  # or just try to start
```

### VM will not start

```bash
# Check KVM availability
ls -la /dev/kvm

# Check FluxVM is reachable
curl -sf http://127.0.0.1:7788/healthz
curl -sf http://127.0.0.1:7788/readyz | jq .
curl -sk https://127.0.0.1:9095/readyz | jq '{ok, store, fluxvm_ok: .fluxvm.ok}'

# Check the VM state and last error
curl -s http://127.0.0.1:9095/api/v1/vms/my-vm \
  -H "Authorization: Bearer $TOKEN" | jq '.last_error'

# Check systemd journal for Zyvor Fabric errors (if running under systemd)
journalctl -u zyvor-fabricd --since "5 min ago" | grep -i error
```

### Authentication failures

```bash
# Read the auto-generated admin password
sudo cat /var/lib/zyvor-fabricd/.admin_password

# Verify JWT secret exists
sudo ls -la /var/lib/zyvor-fabricd/.jwt_secret

# Check auth config
grep -A5 '\[auth\]' /etc/zyvor-fabricd/zyvor-fabricd.toml
```

### Network issues

```bash
# Check bridge exists (managed via direct netlink calls, not
# systemd-networkd -- no networkctl/reload step)
ip link show br0

# List managed network files
curl -s http://127.0.0.1:9095/api/v1/networkd/files \
  -H "Authorization: Bearer $TOKEN" | jq .

# Reload networkd
curl -s -X POST http://127.0.0.1:9095/api/v1/networkd/reload \
  -H "Authorization: Bearer $TOKEN" | jq .
```
