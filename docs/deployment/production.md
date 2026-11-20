# Production Deployment Guide

This document provides a comprehensive guide for deploying Zyvor Fabric in a production
environment. It covers system requirements, installation, configuration, security
hardening, backup strategy, and monitoring setup.

---

## Table of Contents

1. [System Requirements](#system-requirements)
2. [Pre-Installation Checklist](#pre-installation-checklist)
3. [Installation](#installation)
4. [Configuration](#configuration)
5. [Authentication Setup](#authentication-setup)
6. [TLS Configuration](#tls-configuration)
7. [Systemd Service Setup](#systemd-service-setup)
8. [Log Management](#log-management)
9. [Security Hardening](#security-hardening)
10. [Backup Strategy](#backup-strategy)
11. [Monitoring Setup](#monitoring-setup)
12. [Maintenance Procedures](#maintenance-procedures)

---

## System Requirements

### Hardware

| Component | Minimum            | Recommended              |
|-----------|--------------------|--------------------------|
| CPU       | 2 cores (x86_64)   | 8+ cores with VT-x/VT-d |
| RAM       | 4 GB               | 32+ GB                   |
| Disk      | 50 GB              | 500+ GB SSD/NVMe         |
| Network   | 1 Gbps NIC         | 10+ Gbps NIC             |

### Software

| Component           | Minimum Version | Notes                                   |
|---------------------|-----------------|-----------------------------------------|
| Linux Kernel        | 5.15+           | 6.x recommended for best KVM support    |
| QEMU                | 7.0+            | 8.x+ recommended                        |
| KVM                 | Kernel built-in | Verify with `lsmod | grep kvm`          |
| Rust                | 1.75+           | For building from source                 |
| PAM                 | System default  | Required for PAM authentication          |
| SQLite              | 3.35+           | Bundled via rusqlite                     |

zyvor-fabricd's VM lifecycle runs entirely through
[FluxVM](https://github.com/zyvorai/fluxvm) (`driver.fluxvm_url` in
`zyvor-fabricd.toml`), which has no systemd dependency of its own:

| Component           | Minimum Version | Notes                                   |
|---------------------|-----------------|-----------------------------------------|
| FluxVM             | latest          | See [FluxVM's README](https://github.com/zyvorai/fluxvm#readme) for running `fluxvm serve` |

zyvor-fabricd itself (the daemon) has no hard systemd dependency either
— it runs fine as a plain process or under systemd, your choice (see
[systemd-service.md](systemd-service.md)).

### Kernel Modules

Verify the following kernel modules are loaded:

```bash
lsmod | grep -E "kvm|tun|bridge|vhost"
```

Required modules:
- `kvm` and `kvm_intel` or `kvm_amd` -- Hardware virtualization
- `tun` -- TAP networking
- `bridge` -- Network bridging
- `vhost_net` -- Kernel-level virtio networking (optional, improves performance)

---

## Pre-Installation Checklist

```bash
# 1. Verify CPU virtualization support
grep -cE '(vmx|svm)' /proc/cpuinfo

# 2. Verify KVM is available
ls -la /dev/kvm

# 3. Verify the `fluxvm` binary is installed and reachable at the URL
#    configured in zyvor-fabricd.toml (driver.fluxvm_url):
curl -sf "$(grep fluxvm_url /etc/zyvor-fabricd/zyvor-fabricd.toml | cut -d'"' -f2)/healthz"
curl -sf "$(grep fluxvm_url /etc/zyvor-fabricd/zyvor-fabricd.toml | cut -d'"' -f2)/readyz"
# Fabric readiness (store + FluxVM):
curl -sf https://127.0.0.1:9095/readyz
# Tenant filter: GET /api/vms?tenant=acme  (JWT required; JWT tenant claim force-scopes)
# When FluxVM auth is on:
#   [driver]
#   fluxvm_url = "http://127.0.0.1:7788"
#   fluxvm_token = "<matching FluxVM admin token>"
# Optional external Hubble UI link (not in-tree Hubble):
#   [network]
#   hubble_ui_url = "https://hubble.example.com"

# 4. Create the zyvor-fabricd system user (optional, for non-root operation)
sudo useradd --system --home-dir /var/lib/zyvor-fabricd --shell /usr/sbin/nologin zyvor-fabricd

# 5. Ensure the user has access to KVM
sudo usermod -aG kvm zyvor-fabricd
```

---

## Installation

### Building from Source

```bash
# Install build dependencies (Fedora/RHEL)
sudo dnf install -y gcc make openssl-devel pam-devel systemd-devel sqlite-devel

# Install build dependencies (Debian/Ubuntu)
sudo apt install -y build-essential libssl-dev libpam0g-dev libsystemd-dev libsqlite3-dev

# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone and build
git clone https://github.com/zyvorai/fabric.git
cd backend
cargo build --release

# Install binaries
sudo install -m 755 target/release/zyvor-fabricd /usr/local/bin/
sudo install -m 755 target/release/fabricctl /usr/local/bin/
```

### Directory Setup

```bash
# Create required directories
sudo mkdir -p /var/lib/zyvor-fabricd/{images,storage,vms,snapshots,backups,cloud-init,certificates}
sudo mkdir -p /etc/zyvor-fabricd
sudo mkdir -p /var/log/zyvor-fabricd

# Set ownership (if running as the zyvor-fabricd user)
sudo chown -R zyvor-fabricd:zyvor-fabricd /var/lib/zyvor-fabricd
sudo chown -R zyvor-fabricd:zyvor-fabricd /var/log/zyvor-fabricd
```

---

## Configuration

Create the main configuration file at `/etc/zyvor-fabricd/zyvor-fabricd.toml`:

```toml
# =============================================================================
# Zyvor Fabric Production Configuration
# =============================================================================

[daemon]
# Bind address. Use 0.0.0.0 only behind a reverse proxy with TLS.
listen = "127.0.0.1:9095"
# External URL for OpenStack catalog / remote clients (scheme://host:port).
# public_url = "https://zyvor-fabric.example.com"

# CORS origins allowed for the web UI.
# In production, restrict to the actual domain(s) serving the UI.
cors_origins = ["https://zyvor-fabric.example.com"]

[storage]
# Root directory for all persistent state.
path = "/var/lib/zyvor-fabricd"

# Directory for VM disk images.
image_path = "/var/lib/zyvor-fabricd/images"

[network]
# Default bridge for VM networking.
bridge = "br0"

# `networkd_config_dir`/`networkd_file_prefix` are legacy config fields —
# host networking (bridges/VLANs/bonds/VXLAN, WireGuard mesh) now goes
# through netlink directly rather than writing systemd-networkd
# .netdev/.network files, so these no longer affect anything and can be
# left at their defaults.

[auth]
# Enable authentication (strongly recommended for production).
enabled = true

# JWT signing secret. Set this explicitly or use the environment variable
# ZYVOR_FABRICD_JWT_SECRET. If not set, a random secret is generated and
# persisted to /var/lib/zyvor-fabricd/.jwt_secret.
# jwt_secret = "your-secure-random-secret-here"

# Path to the SQLite user database.
db_path = "/var/lib/zyvor-fabricd/auth.db"

# JWT token expiration in hours.
token_expiration_hours = 8

[controller]
# Enable controller mode for multi-host management.
enabled = false
mode = "standalone"
# cluster_name = "production"
# datacenter_name = "dc-east-1"
```

### Environment Variables

| Variable                  | Description                             | Default              |
|---------------------------|-----------------------------------------|----------------------|
| `ZYVOR_FABRICD_JWT_SECRET`     | JWT signing secret                      | Auto-generated       |
| `ZYVOR_FABRICD_ADMIN_PASSWORD` | Initial admin user password             | Auto-generated       |
| `ZYVOR_FABRICD_LOG_LEVEL`        | Log level (trace/debug/info/warn/error) | `info`               |
| `RUST_LOG`                | Standard Rust log filter (fallback)     | `Zyvor Fabric=info`      |

---

## Authentication Setup

### Initial Admin Access

On first startup with authentication enabled, Zyvor Fabric creates an `admin` user:

1. If `ZYVOR_FABRICD_ADMIN_PASSWORD` is set, that password is used
2. Otherwise, a random password is generated and written to
   `/var/lib/zyvor-fabricd/.admin_password` (mode 0600)

```bash
# Read the generated admin password
sudo cat /var/lib/zyvor-fabricd/.admin_password

# Log in and obtain a JWT token
curl -s http://127.0.0.1:9095/api/v1/auth/sign-in \
  -H "Content-Type: application/json" \
  -d '{"username": "admin", "password": "THE_PASSWORD"}'
```

### Creating Additional Users

```bash
TOKEN="your-admin-jwt-token"

# Create a regular user
curl -s http://127.0.0.1:9095/api/v1/auth/users \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"username": "operator", "password": "secure-password", "role": "user"}'

# Create a read-only viewer
curl -s http://127.0.0.1:9095/api/v1/auth/users \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"username": "auditor", "password": "secure-password", "role": "viewer"}'
```

### PAM Authentication

Zyvor Fabric can authenticate against the system PAM stack. Create a PAM service file:

```bash
sudo tee /etc/pam.d/zyvor-fabricd << 'EOF'
auth    required    pam_unix.so
account required    pam_unix.so
EOF
```

When this file exists, Zyvor Fabric will use the `Zyvor Fabric` PAM service for
authentication. Otherwise, it falls back to the `login` PAM service.

---

## TLS Configuration

Zyvor Fabric does not terminate TLS directly. Use a reverse proxy for TLS termination.

### nginx Configuration

```nginx
upstream zyvor_fabricd {
    server 127.0.0.1:9095;
}

server {
    listen 443 ssl http2;
    server_name zyvor-fabric.example.com;

    ssl_certificate     /etc/ssl/certs/zyvor-fabricd.crt;
    ssl_certificate_key /etc/ssl/private/zyvor-fabricd.key;
    ssl_protocols       TLSv1.2 TLSv1.3;
    ssl_ciphers         HIGH:!aNULL:!MD5;

    # REST API and static web UI
    location / {
        proxy_pass http://zyvor_fabricd;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }

    # WebSocket console
    location /api/v1/ws/ {
        proxy_pass http://zyvor_fabricd;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_read_timeout 3600s;
        proxy_send_timeout 3600s;
    }

    # SSE event stream
    location /api/v1/events/stream {
        proxy_pass http://zyvor_fabricd;
        proxy_set_header Connection '';
        proxy_http_version 1.1;
        proxy_buffering off;
        proxy_cache off;
        chunked_transfer_encoding off;
    }
}
```

---

## Systemd Service Setup (Optional)

zyvor-fabricd doesn't require systemd — packages ship `zyvor-fabricd.service` but don't enable or start it automatically. If you want to run it under systemd, see [systemd-service.md](systemd-service.md) for the complete unit file. Quick setup:

```bash
sudo install -m 644 systemd/zyvor-fabricd.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now zyvor-fabricd
```

---

## Log Management

### Journald Integration

Zyvor Fabric logs to stdout/stderr, which systemd captures into the journal:

```bash
# View live logs
journalctl -u Zyvor Fabric -f

# View logs since last boot
journalctl -u Zyvor Fabric -b

# View only errors
journalctl -u Zyvor Fabric -p err

# Export logs for analysis
journalctl -u Zyvor Fabric --since "1 hour ago" -o json > Zyvor Fabric-logs.json
```

### Log Levels

Set the log level via the `ZYVOR_FABRICD_LOG_LEVEL` environment variable in the
systemd unit file or on the command line:

| Level   | Description                                      |
|---------|--------------------------------------------------|
| `error` | Critical errors only                             |
| `warn`  | Warnings and errors                              |
| `info`  | Normal operational messages (default)            |
| `debug` | Detailed debugging information                   |
| `trace` | Very verbose trace-level logging                 |

### Audit Logs

Zyvor Fabric writes structured audit log entries for all state-changing operations.
Query audit logs via the API:

```bash
curl -s http://127.0.0.1:9095/api/v1/audit/logs \
  -H "Authorization: Bearer $TOKEN" | jq .

# Export audit logs
curl -s http://127.0.0.1:9095/api/v1/audit/logs/export \
  -H "Authorization: Bearer $TOKEN" > audit-export.json
```

---

## Security Hardening

### Checklist

- [ ] Enable authentication (`auth.enabled = true`)
- [ ] Set an explicit JWT secret via `ZYVOR_FABRICD_JWT_SECRET` environment variable
- [ ] Reduce token expiration (`token_expiration_hours = 8` or less)
- [ ] Assign `users.tenant` for tenant-scoped operators (JWT claim enforces create/list/get/mutate)
- [ ] When FluxVM has `[[auth.tokens]]`, set `driver.fluxvm_token` to a matching admin bearer
- [ ] Bind to localhost only (`listen = "127.0.0.1:9095"`)
- [ ] Use a reverse proxy with TLS for external access
- [ ] Restrict CORS origins to your actual domain(s)
- [ ] Create a dedicated system user for Zyvor Fabric
- [ ] Apply the systemd sandboxing options from [systemd-service.md](systemd-service.md)
- [ ] Set restrictive file permissions on `/var/lib/zyvor-fabricd/` (mode 0700)
- [ ] Rotate the JWT secret periodically
- [ ] Monitor audit logs for unauthorized access attempts
- [ ] Enable resource quotas to prevent resource exhaustion
- [ ] Configure firewall rules to restrict management port access
- [ ] Optional: `network.hubble_ui_url` for an external Hubble link ([hubble-ui.md](../guides/operations/hubble-ui.md))

### File Permissions

```bash
# Lock down the data directory
sudo chmod 700 /var/lib/zyvor-fabricd
sudo chmod 600 /var/lib/zyvor-fabricd/.jwt_secret
sudo chmod 600 /var/lib/zyvor-fabricd/.admin_password
sudo chmod 600 /var/lib/zyvor-fabricd/auth.db

# Lock down the config directory
sudo chmod 750 /etc/zyvor-fabricd
sudo chmod 640 /etc/zyvor-fabricd/zyvor-fabricd.toml
```

### Network Security

```bash
# Allow management access only from trusted networks
sudo firewall-cmd --zone=internal --add-rich-rule='
  rule family="ipv4" source address="10.0.0.0/8"
  port protocol="tcp" port="9095" accept'

# Or with nftables directly
sudo nft add rule inet filter input ip saddr 10.0.0.0/8 tcp dport 9095 accept
sudo nft add rule inet filter input tcp dport 9095 drop
```

---

## Backup Strategy

### What to Back Up

| Path                           | Priority | Contents                          |
|--------------------------------|----------|-----------------------------------|
| `/etc/zyvor-fabricd/zyvor-fabricd.toml`  | Critical | Daemon configuration              |
| `/var/lib/zyvor-fabricd/auth.db`    | Critical | User accounts and passwords       |
| `/var/lib/zyvor-fabricd/.jwt_secret`| Critical | JWT signing key                   |
| `/var/lib/zyvor-fabricd/vms/`       | High     | VM state metadata                 |
| `/var/lib/zyvor-fabricd/images/`    | High     | VM disk images (large)            |
| `/var/lib/zyvor-fabricd/snapshots/` | Medium   | Snapshot metadata                 |
| `/var/lib/zyvor-fabricd/certificates/` | Medium | TLS certificates and CAs       |
| `/etc/systemd/network/50-Zyvor Fabric-*` | Low | Generated network configs (recreatable) |

### Backup Script Example

```bash
#!/bin/bash
# Zyvor Fabric-backup.sh - Daily backup script
set -euo pipefail

BACKUP_DIR="/backup/zyvor-fabricd/$(date +%Y-%m-%d)"
mkdir -p "$BACKUP_DIR"

# Stop the daemon briefly for consistent backup (optional)
# sudo systemctl stop zyvor-fabricd

# Back up configuration and state
sudo tar czf "$BACKUP_DIR/config.tar.gz" /etc/zyvor-fabricd/
sudo tar czf "$BACKUP_DIR/state.tar.gz" \
  /var/lib/zyvor-fabricd/auth.db \
  /var/lib/zyvor-fabricd/.jwt_secret \
  /var/lib/zyvor-fabricd/vms/ \
  /var/lib/zyvor-fabricd/snapshots/ \
  /var/lib/zyvor-fabricd/certificates/

# Back up VM images (incremental with rsync)
sudo rsync -a --delete /var/lib/zyvor-fabricd/images/ "$BACKUP_DIR/images/"

# Restart if stopped
# sudo systemctl start zyvor-fabricd

# Retain 30 days of backups
find /backup/zyvor-fabricd/ -maxdepth 1 -type d -mtime +30 -exec rm -rf {} +

echo "Backup completed: $BACKUP_DIR"
```

### Automated Backup via API

Zyvor Fabric provides a built-in backup API:

```bash
# Create a backup
curl -s -X POST http://127.0.0.1:9095/api/v1/backups \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"vm_name": "production-db", "description": "Pre-upgrade backup"}'

# Create a backup policy (daily at 2 AM)
curl -s -X POST http://127.0.0.1:9095/api/v1/backups/policies \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"vm_name": "production-db", "schedule": "0 2 * * *", "retention_days": 30}'
```

---

## Monitoring Setup

### Liveness and readiness

```bash
curl -sf https://127.0.0.1:9095/health          # liveness
curl -sf https://127.0.0.1:9095/readyz | jq .   # readiness (store + FluxVM)
```

**Easiest install** — ship FluxVM + Fabric together:

```bash
./scripts/ship sus@HOST
# or first install:
./scripts/ship sus@HOST --full
```

**Production readiness script** (read-only; doctor + devops + dataplane/VPN list APIs) — also run automatically by `ship`:

```bash
FABRIC_URL=https://127.0.0.1:9095 \
FLUXVM_URL=http://127.0.0.1:7788 \
FABRIC_TOKEN=<admin-or-ops-jwt> \
  ./scripts/test-production-readiness.sh
```

Mutating WireGuard / edge e2e only with `RUN_MUTATING=1`. Pair with FluxVM
[`scripts/test-production-readiness.sh`](https://github.com/zyvorai/fluxvm/blob/main/scripts/test-production-readiness.sh)
on each dataplane host. Prod ship: `FABRIC_ADMIN_PASSWORD=… ./scripts/ship sus@HOST --prod`.

Use `/health` for process-up probes and `/readyz` for load-balancer / Kubernetes
readiness (DaemonSet templates already probe `/readyz`; FluxVM uses `/healthz` +
`/readyz`). See also [monitoring.md](../guides/operations/monitoring.md).

### Prometheus Integration

Zyvor Fabric exposes a Prometheus-compatible metrics endpoint:

```bash
curl http://127.0.0.1:9095/metrics
```

Available metrics:

| Metric                        | Type    | Description                    |
|-------------------------------|---------|--------------------------------|
| `zyvor_fabricd_vms_total`          | Gauge   | Total number of VMs            |
| `zyvor_fabricd_vms_running`        | Gauge   | Number of running VMs          |
| `zyvor_fabricd_vms_stopped`        | Gauge   | Number of stopped VMs          |
| `zyvor_fabricd_vm_starts_total`    | Counter | Total VM start operations      |
| `zyvor_fabricd_vm_stops_total`     | Counter | Total VM stop operations       |
| `zyvor_fabricd_vm_creates_total`   | Counter | Total VM create operations     |
| `zyvor_fabricd_vm_deletes_total`   | Counter | Total VM delete operations     |

### Prometheus Configuration

```yaml
# prometheus.yml
scrape_configs:
  - job_name: 'Zyvor Fabric'
    scrape_interval: 15s
    static_configs:
      - targets: ['127.0.0.1:9095']
    metrics_path: '/metrics'
```

### Health Check

Use the VM list endpoint as a basic health check:

```bash
# Simple health check
curl -sf http://127.0.0.1:9095/api/v1/vms > /dev/null && echo "OK" || echo "FAIL"
```

### Alerting Rules (Prometheus)

```yaml
# Zyvor Fabric-alerts.yml
groups:
  - name: Zyvor Fabric
    rules:
      - alert: ZyvorFabricdDown
        expr: up{job="Zyvor Fabric"} == 0
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "Zyvor Fabric is unreachable"

      - alert: HighVMCount
        expr: zyvor_fabricd_vms_running > 100
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High number of running VMs ({{ $value }})"
```

---

## Maintenance Procedures

### Rolling Restart

```bash
# Graceful restart (in-flight requests complete, background tasks shut down)
sudo systemctl restart zyvor-fabricd
```

### Upgrading

```bash
# 1. Build new version
cd backend && cargo build --release

# 2. Stop the daemon
sudo systemctl stop zyvor-fabricd

# 3. Back up state
sudo tar czf /tmp/zyvor-fabricd-pre-upgrade.tar.gz /var/lib/zyvor-fabricd/

# 4. Install new binary
sudo install -m 755 target/release/zyvor-fabricd /usr/local/bin/

# 5. Start the daemon
sudo systemctl start zyvor-fabricd

# 6. Verify
journalctl -u zyvor-fabricd -n 20
curl -sf http://127.0.0.1:9095/api/v1/vms | jq .total
```

### Database Maintenance

The SQLite user database requires minimal maintenance. To compact it:

```bash
sudo systemctl stop zyvor-fabricd
sudo sqlite3 /var/lib/zyvor-fabricd/auth.db "VACUUM;"
sudo systemctl start zyvor-fabricd
```
