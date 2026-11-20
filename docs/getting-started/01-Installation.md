# Installation Guide

This guide covers installing Zyvor Fabric on a Linux system, along with QEMU/KVM for virtual machine execution. Zyvor Fabric's VM lifecycle is entirely owned by [FluxVM](https://github.com/zyvorai/fluxvm) (`driver.fluxvm_url` in `zyvor-fabricd.toml`), which has no systemd version requirement at all. Zyvor Fabric itself (the daemon) doesn't require systemd either — it can run under systemd or as a plain process.

---

## System Requirements

### Hardware

| Resource | Minimum | Recommended |
|----------|---------|-------------|
| CPU | 2 cores (x86_64) | 4+ cores with VT-x/VT-d |
| Memory | 4 GB RAM | 8+ GB RAM |
| Disk | 20 GB free | 100+ GB (SSD recommended) |
| Network | 1 NIC | 2+ NICs for bridge networking |

### Software

| Requirement | Minimum Version | Notes |
|-------------|-----------------|-------|
| Linux kernel | 5.15+ | x86_64 architecture |
| FluxVM | latest | See [FluxVM's README](https://github.com/zyvorai/fluxvm#readme) for running `fluxvm serve` |
| QEMU | 8.0+ | KVM acceleration recommended |
| Rust | 1.75+ | Only for building from source |

### Supported Distributions

Since VM lifecycle has no systemd version requirement, any current Linux distribution with kernel 5.15+ and KVM support works — there's no systemd-version floor to check anymore.

---

## Fedora Installation

Fedora 41 and later ship with systemd 256+ and have full support for Zyvor Fabric.

### 1. Install Dependencies

```bash
sudo dnf install -y \
    qemu-kvm \
    qemu-img \
    swtpm \
    swtpm-tools \
    edk2-ovmf \
    genisoimage \
    bridge-utils \
    wireguard-tools \
    nftables
```

### 2. Verify FluxVM is reachable

```bash
curl -sf http://127.0.0.1:7788/healthz
curl -sf http://127.0.0.1:7788/readyz | jq .
```

See [FluxVM's README](https://github.com/zyvorai/fluxvm#readme) and
[PRODUCTION.md](https://github.com/zyvorai/fluxvm/blob/main/docs/PRODUCTION.md)
for running `fluxvm serve` if this doesn't succeed yet.

### 3. Enable KVM

```bash
# Verify KVM is available
lsmod | grep kvm

# If not loaded, load the appropriate module
sudo modprobe kvm_intel   # Intel processors
sudo modprobe kvm_amd     # AMD processors

# Verify /dev/kvm exists
ls -la /dev/kvm
```

### 4. Deploy Zyvor Fabric

```bash
git clone <repository-url>
cd zyvor-fabric

# One-command deployment
./zyvor-fabricd-ctl deploy
```

This command will:
- Install any missing system dependencies
- Build the Rust workspace (53 crates)
- Install binaries (`Zyvor Fabric`, `fabricctl`) to `/usr/local/bin/`
- Install the systemd service unit
- Create configuration directories
- Start the zyvor-fabricd service
- Run a post-install verification

---

## Ubuntu / Debian Installation

### 1. Install Dependencies

```bash
sudo apt update
sudo apt install -y \
    qemu-kvm \
    qemu-utils \
    swtpm \
    swtpm-tools \
    ovmf \
    genisoimage \
    bridge-utils \
    wireguard-tools \
    nftables
```

### 2. Verify FluxVM is reachable

```bash
curl -sf http://127.0.0.1:7788/healthz
curl -sf http://127.0.0.1:7788/readyz | jq .
```

See [FluxVM's README](https://github.com/zyvorai/fluxvm#readme) for running `fluxvm serve` if this doesn't succeed yet.

### 3. Enable KVM

```bash
# Verify KVM support
sudo apt install -y cpu-checker
kvm-ok

# Load KVM module if needed
sudo modprobe kvm_intel   # Intel
sudo modprobe kvm_amd     # AMD
```

### 4. Deploy Zyvor Fabric

```bash
git clone <repository-url>
cd zyvor-fabric
./zyvor-fabricd-ctl deploy
```

---

## Building from Source

If you prefer to build manually or are developing on the project, follow these steps.

### 1. Install the Rust Toolchain

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# Verify installation
rustc --version
cargo --version
```

The minimum supported Rust version is 1.75.

### 2. Clone the Repository

```bash
git clone <repository-url>
cd zyvor-fabric
```

### 3. Build the Workspace

```bash
cd backend

# Fast compile check (no binary output)
cargo check

# Full release build
cargo build --release
```

The release build produces optimized binaries in `backend/target/release/`.

### 4. Run Tests

```bash
cd backend
cargo test
```

All tests must pass with zero warnings before deployment.

### 5. Install Binaries

```bash
# Install the daemon
sudo cp backend/target/release/zyvor-fabricd /usr/local/bin/

# Install the CLI
sudo cp backend/target/release/fabricctl /usr/local/bin/

# Create config directory
sudo mkdir -p /etc/zyvor-fabricd

# Create state directory
sudo mkdir -p /var/lib/zyvor-fabricd/images
sudo mkdir -p /var/lib/zyvor-fabricd/storage
sudo mkdir -p /var/lib/zyvor-fabricd/cloud-init
```

### 6. Install the systemd Service (Optional)

Zyvor Fabric doesn't require systemd — you can run the binary directly, or
under any other supervisor. If you'd rather run it under systemd, create
the service unit file (the repo also ships one at `systemd/zyvor-fabricd.service`
you can install as-is instead of typing this out):

```bash
sudo tee /etc/systemd/system/zyvor-fabricd.service > /dev/null << 'EOF'
[Unit]
Description=Zyvor Fabric VM Management Daemon
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=/usr/local/bin/zyvor-fabricd
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF
```

Enable and start:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now zyvor-fabricd
```

---

## Verifying the Installation

### 1. Check Service Status

If you're running it under systemd (step 6 above):

```bash
sudo systemctl status zyvor-fabricd
```

You should see `active (running)`. If you're running it directly or under
another supervisor, check that with whatever you're using instead.

### 2. Check the API

```bash
curl -s http://127.0.0.1:9095/api/vms | jq .
```

A successful response returns a JSON object with an empty VM list.

### 3. Read the Admin Password

On first start, Zyvor Fabric generates a random admin password:

```bash
# Using zyvor-fabricd-ctl
./zyvor-fabricd-ctl password

# Or read directly
sudo cat /var/lib/zyvor-fabricd/.admin_password
```

### 4. Run the Smoke Test

```bash
./zyvor-fabricd-ctl verify
```

This runs automated checks against the API, authentication, VM CRUD operations, and backup functionality.

### 5. Run the Health Check

```bash
./zyvor-fabricd-ctl health
```

This performs a deep check of API availability, disk space, database integrity, credential files, the scheduled backup/cleanup task, memory, and KVM support.

---

## Troubleshooting

### Cannot connect to FluxVM

VM operations will fail if `fluxvm serve` isn't running or isn't reachable at the URL configured in `zyvor-fabricd.toml` (`driver.fluxvm_url`, default `http://127.0.0.1:7788`):

```bash
curl -sf http://127.0.0.1:7788/healthz
curl -sf http://127.0.0.1:7788/readyz | jq .
```

If that fails, see [FluxVM's README](https://github.com/zyvorai/fluxvm#readme) for starting it.

### KVM not available

Ensure hardware virtualization is enabled in your BIOS/UEFI settings (Intel VT-x or AMD-V). Then load the kernel module:

```bash
sudo modprobe kvm_intel   # or kvm_amd
```

### Permission denied on /dev/kvm

Add your user to the `kvm` group:

```bash
sudo usermod -aG kvm $USER
# Log out and back in for the change to take effect
```

### Port 9095 already in use

Change the listen address in the config file (any free port — example uses
`19095`). Also set `public_url` if OpenStack clients should see the same port:

```bash
sudo mkdir -p /etc/zyvor-fabricd
sudo tee /etc/zyvor-fabricd/zyvor-fabricd.toml > /dev/null << 'EOF'
[daemon]
listen = "127.0.0.1:19095"
public_url = "https://127.0.0.1:19095"
EOF

sudo systemctl restart zyvor-fabricd
```

Or: `export ZYVOR_FABRICD_LISTEN=127.0.0.1:19095` and
`export ZYVOR_FABRICD_PUBLIC_URL=https://127.0.0.1:19095`.

---

## Next Steps

- [Quick Start](02-Quick-Start.md) -- create your first VM
- [Configuration Reference](03-Configuration.md) -- customize Zyvor Fabric settings
- [Web UI Guide](04-Web-UI.md) -- access the web dashboard
