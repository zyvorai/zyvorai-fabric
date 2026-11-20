# Configuration Reference

Zyvor Fabric is configured through a TOML configuration file and environment variables. This guide covers all configuration sections and options.

---

## Config File Locations

Zyvor Fabric searches for its configuration file in the following order:

| Priority | Path | Use Case |
|----------|------|----------|
| 1 | `/etc/zyvor-fabricd/zyvor-fabricd.toml` | Production deployment |
| 2 | `configs/zyvor-fabricd.toml` | Development (relative to working directory) |
| 3 | `zyvor-fabricd.toml` | Development (current directory) |

If no config file is found, Zyvor Fabric uses built-in defaults and logs a warning.

---

## Complete Configuration Example

```toml
[daemon]
listen = "127.0.0.1:9095"
# public_url = "https://fabric.example.com:9095"
cors_origins = ["http://127.0.0.1:9095"]

[storage]
path = "/var/lib/zyvor-fabricd"
image_path = "/var/lib/zyvor-fabricd/images"

[network]
bridge = "br0"
networkd_config_dir = "/etc/systemd/network"
networkd_file_prefix = "50-Zyvor Fabric-"

[auth]
enabled = true
db_path = "/var/lib/zyvor-fabricd/auth.db"
token_expiration_hours = 24

[controller]
enabled = false
mode = "standalone"
```

---

## Configuration Sections

### [daemon]

Controls the HTTP server and API behavior.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `listen` | String | `"127.0.0.1:9095"` | Address and port for the HTTP server |
| `cors_origins` | Array of Strings | `["http://127.0.0.1:9095"]` | Allowed CORS origins for web UI access |
| `public_url` | String (optional) | unset | External base URL advertised to clients (OpenStack catalog). Override with `ZYVOR_FABRICD_PUBLIC_URL`. When unset, derived from `listen` + TLS (`0.0.0.0` → `127.0.0.1`). |

#### Listen Address

The `listen` address determines which network interfaces the API server binds to:

```toml
[daemon]
# Localhost only (default, most secure)
listen = "127.0.0.1:9095"

# All interfaces (required for remote access)
listen = "0.0.0.0:9095"

# Specific interface
listen = "192.168.1.100:9095"

# Custom port (also set public_url so OpenStack clients see the same host:port)
listen = "127.0.0.1:19095"
public_url = "https://127.0.0.1:19095"
```

> **Security note:** Binding to `0.0.0.0` exposes the API to the network. Always enable authentication and consider TLS when using a non-localhost address.

#### Public URL

OpenStack service-catalog endpoints and similar client-facing URLs must match
what callers use. Prefer an explicit value for remote hosts, reverse proxies,
and Kubernetes NodePort/ingress:

```toml
[daemon]
listen = "0.0.0.0:9095"
public_url = "https://fabric.example.com:9095"
```

```bash
export ZYVOR_FABRICD_PUBLIC_URL=https://fabric.example.com:9095
export ZYVOR_FABRICD_LISTEN=0.0.0.0:9095
```

See [openstack-compat.md](../openstack-compat.md) and
[Tutorial 08](../tutorials/08-openstack-clients.md).

#### CORS Configuration

The `cors_origins` list controls which web origins can access the API. This is required for the web dashboard when it runs on a different origin than the API.

```toml
[daemon]
cors_origins = [
    "http://127.0.0.1:9095",
    "http://localhost:3000",
    "https://vm-dashboard.example.com"
]
```

Invalid origins are logged as warnings and ignored at startup. The API allows the following HTTP methods across origins: `GET`, `POST`, `PUT`, `DELETE`, `OPTIONS`. The `Content-Type` and `Authorization` headers are permitted.

---

### [storage]

Controls where VM state, disk images, and storage pools are located.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `path` | String | `"/var/lib/zyvor-fabricd"` | Root directory for state data |
| `image_path` | String | `"/var/lib/zyvor-fabricd/images"` | Directory for VM disk images |

```toml
[storage]
path = "/var/lib/zyvor-fabricd"
image_path = "/var/lib/zyvor-fabricd/images"
```

The `path` directory stores:
- The SQLite state database
- Authentication database
- JWT secret and admin password files
- Cloud-init configuration data

The `image_path` directory stores:
- VM disk images (qcow2, raw)
- Downloaded cloud images
- ISO files

Storage pool data is stored under `/var/lib/zyvor-fabricd/storage/` by default.

---

### [network]

Controls the default network bridge and optional discovery of host-managed systemd-networkd files.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `bridge` | String | `"br0"` | Default network bridge for VMs |
| `networkd_config_dir` | String | `"/etc/systemd/network"` | Directory scanned to *discover* pre-existing, host-managed systemd-networkd files (read-only) |
| `networkd_file_prefix` | String | `"50-zyvor-fabricd-"` | Prefix historically used to distinguish zyvor-fabricd-managed files from manually created ones |

```toml
[network]
bridge = "br0"
networkd_config_dir = "/etc/systemd/network"
networkd_file_prefix = "50-zyvor-fabricd-"
```

Zyvor Fabric creates and configures bridges, VLANs, bonds, TAP/macvtap devices, VXLANs, and addresses directly via netlink (`rtnetlink`) — it does not write `.netdev`/`.network` files and needs no `networkctl reload` step. `networkd_config_dir` is only used to *discover* `.netdev`/`.network`/`.link` files that a host's own independent systemd-networkd setup already manages, so they can be shown (read-only) alongside zyvor-fabricd-managed devices in the network API; it plays no role in how zyvor-fabricd itself configures networking.

---

### [auth]

Controls authentication and authorization.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | Bool | `true` | Enable JWT authentication |
| `jwt_secret` | String | Auto-generated | Secret key for signing JWT tokens |
| `db_path` | String | `"/var/lib/zyvor-fabricd/auth.db"` | Path to the SQLite user database |
| `default_admin_password` | String | Auto-generated | Initial admin password |
| `token_expiration_hours` | Integer | `24` | JWT token validity period in hours |

```toml
[auth]
enabled = true
db_path = "/var/lib/zyvor-fabricd/auth.db"
token_expiration_hours = 24
```

> **Note:** The `jwt_secret` and `default_admin_password` fields are never serialized to disk. They are read from environment variables, persisted files, or auto-generated.

#### JWT Secret Management

The JWT signing secret is resolved in the following order:

1. **Environment variable** `ZYVOR_FABRICD_JWT_SECRET` (highest priority)
2. **Persisted file** `/var/lib/zyvor-fabricd/.jwt_secret` (survives restarts)
3. **Auto-generated** random 64-character string (written to the persisted file)

The persisted file is created with `0600` permissions (owner-only read/write).

For production deployments, set the secret explicitly:

```bash
export ZYVOR_FABRICD_JWT_SECRET="your-secure-random-string-at-least-64-chars"
```

#### Admin Password Management

The default admin password follows the same resolution order:

1. **Environment variable** `ZYVOR_FABRICD_ADMIN_PASSWORD` (highest priority)
2. **Persisted file** `/var/lib/zyvor-fabricd/.admin_password` (survives restarts)
3. **Auto-generated** random 64-character string (written to the persisted file)

The password file is created with `0600` permissions. If permissions cannot be set, the file is deleted for security.

Read the current admin password:

```bash
sudo cat /var/lib/zyvor-fabricd/.admin_password
# Or using fabricctl
./zyvor-fabricd-ctl password
```

For production, set a known password:

```bash
export ZYVOR_FABRICD_ADMIN_PASSWORD="your-secure-admin-password"
```

#### Disabling Authentication

For development or testing, authentication can be disabled:

```toml
[auth]
enabled = false
```

> **Warning:** Disabling authentication removes all access control. Never disable authentication in production.

#### RBAC Roles

Zyvor Fabric enforces three-tier role-based access control on every API endpoint:

| Role | Permissions | Typical Use |
|------|-------------|-------------|
| `admin` | Full access (read + write + admin operations) | System administrators |
| `user` | Read + write (create, start, stop VMs) | Developers, operators |
| `viewer` | Read-only (list VMs, view metrics) | Monitoring, dashboards |

Admin-only operations include: deleting VMs, managing users, modifying system settings.

---

### [controller]

Controls multi-node clustering and controller mode.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | Bool | `false` | Enable controller/clustering features |
| `mode` | String | `"standalone"` | Deployment mode: `standalone` or `controller` |
| `cluster_name` | String | None | Name of the cluster |
| `datacenter_name` | String | None | Name of the datacenter |

```toml
[controller]
enabled = true
mode = "controller"
cluster_name = "production"
datacenter_name = "us-east-1"
```

In `standalone` mode (default), Zyvor Fabric runs as a single node. In `controller` mode, it participates in cluster coordination with etcd-based leader election and distributed resource scheduling.

---

## Environment Variables

Environment variables override config file values for sensitive settings.

| Variable | Overrides | Description |
|----------|-----------|-------------|
| `ZYVOR_FABRICD_LISTEN` | `daemon.listen` | Bind address (`host:port`, default port **9095**) |
| `ZYVOR_FABRICD_PUBLIC_URL` | `daemon.public_url` | External base URL for OpenStack catalog / clients |
| `ZYVOR_FABRICD_CONFIG` | — | Config file path |
| `ZYVOR_FABRICD_LOG_LEVEL` | — | Log level |
| `ZYVOR_FABRICD_JWT_SECRET` | `auth.jwt_secret` | JWT signing secret |
| `ZYVOR_FABRICD_ADMIN_PASSWORD` | `auth.default_admin_password` | Default admin password |
| `ZYVOR_FABRICD_BACKUP_DIR` | Backup directory | Override backup storage location |
| `ZYVOR_FABRICD_BACKUP_RETAIN` | Backup retention | Number of backups to retain |
| `ZYVOR_FABRICD_BACKUP_TYPE` | Backup type | Backup format/type |

Set environment variables in the systemd service unit for production:

```bash
sudo systemctl edit zyvor-fabricd
```

Add:

```ini
[Service]
Environment="ZYVOR_FABRICD_JWT_SECRET=your-secret-here"
Environment="ZYVOR_FABRICD_ADMIN_PASSWORD=your-password-here"
```

Then reload and restart:

```bash
sudo systemctl daemon-reload
sudo systemctl restart zyvor-fabricd
```

---

## File Permissions

Zyvor Fabric creates several files with restrictive permissions:

| File | Permissions | Contents |
|------|-------------|----------|
| `/var/lib/zyvor-fabricd/.jwt_secret` | `0600` | JWT signing key |
| `/var/lib/zyvor-fabricd/.admin_password` | `0600` | Admin password |
| `/var/lib/zyvor-fabricd/auth.db` | `0600` | User database (bcrypt hashes) |

These files should only be readable by the user running the Zyvor Fabric process (typically root).

---

### [auth.totp]

Controls two-factor authentication (2FA) via TOTP (Time-based One-Time Password).

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | Bool | `false` | Enable 2FA/TOTP support |
| `issuer` | String | `"Zyvor Fabric"` | Issuer name shown in authenticator apps |
| `digits` | Integer | `6` | Number of digits in the TOTP code |
| `period` | Integer | `30` | TOTP code validity period in seconds |

```toml
[auth.totp]
enabled = true
issuer = "Zyvor Fabric"
digits = 6
period = 30
```

When enabled, users can set up 2FA via `POST /api/v1/auth/2fa/setup`, which returns a TOTP secret and a provisioning URI for authenticator apps (Google Authenticator, Authy, etc.). Once verified, subsequent logins require a `totp_code` field in addition to the username and password.

---

### [storage.iscsi]

Controls iSCSI storage backend integration.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | Bool | `false` | Enable iSCSI storage backend |
| `initiator_name` | String | Auto-detected | iSCSI initiator IQN |
| `default_port` | Integer | `3260` | Default iSCSI target port |
| `chap_username` | String | None | CHAP authentication username |
| `chap_secret` | String | None | CHAP authentication secret |
| `discovery_timeout` | Integer | `10` | Target discovery timeout in seconds |

```toml
[storage.iscsi]
enabled = true
initiator_name = "iqn.2026-01.com.example:Zyvor Fabric"
default_port = 3260
chap_username = "Zyvor Fabric"
chap_secret = "your-chap-secret"
discovery_timeout = 10
```

> **Security note:** Store the `chap_secret` via environment variables or the secrets manager rather than in the config file.

---

### [network.dhcp]

Controls the built-in DHCP server on bridge interfaces.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | Bool | `false` | Enable DHCP server on managed bridges |
| `lease_time` | String | `"1h"` | Default DHCP lease duration |
| `dns_servers` | Array of Strings | `[]` | DNS servers advertised to clients |
| `domain` | String | None | DNS search domain for DHCP clients |

```toml
[network.dhcp]
enabled = true
lease_time = "1h"
dns_servers = ["8.8.8.8", "8.8.4.4"]
domain = "vm.internal"
```

The DHCP server is a directly-managed `dnsmasq` process per bridge (via `zyvor-fabric-dnsmasq-manager`, not systemd-networkd's built-in `[DHCPServer]`) and assigns addresses from the pool range configured on each bridge via the API (see `POST /api/networkd/dhcp`).

---

### [compliance]

Controls default compliance scanning behavior.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | Bool | `false` | Enable compliance scanning |
| `default_profile` | String | `"cis-level1"` | Default compliance profile for new VMs |
| `auto_scan` | Bool | `false` | Automatically scan VMs on creation |
| `scan_interval_hours` | Integer | `24` | Interval between automatic scans |
| `fail_on_critical` | Bool | `true` | Prevent VM start if critical findings exist |

```toml
[compliance]
enabled = true
default_profile = "cis-level1"
auto_scan = true
scan_interval_hours = 24
fail_on_critical = true
```

Available built-in profiles: `cis-level1`, `cis-level2`, `stig`, `pci-dss`, `hipaa`. Custom profiles can be created via the API.

---

### [billing]

Controls billing and chargeback defaults.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | Bool | `false` | Enable billing and usage tracking |
| `currency` | String | `"USD"` | Default currency for pricing |
| `billing_cycle` | String | `"monthly"` | Billing cycle: `hourly`, `daily`, `monthly` |
| `cpu_rate` | Float | `0.01` | Price per vCPU per hour |
| `memory_rate` | Float | `0.005` | Price per GB of memory per hour |
| `storage_rate` | Float | `0.0001` | Price per GB of storage per hour |
| `network_rate` | Float | `0.001` | Price per GB of network transfer |

```toml
[billing]
enabled = true
currency = "USD"
billing_cycle = "monthly"
cpu_rate = 0.01
memory_rate = 0.005
storage_rate = 0.0001
network_rate = 0.001
```

When enabled, Zyvor Fabric meters resource usage per VM and generates invoices on the configured billing cycle. Usage data is accessible via `GET /api/v1/billing/usage` and invoices via `GET /api/v1/billing/invoices`.

---

## Next Steps

- [Web UI Guide](04-Web-UI.md) -- access the web dashboard
- [Security Guide](../security.md) -- RBAC, API keys, and audit logging
- [Networking Guide](../networking.md) -- network bridge and policy configuration
