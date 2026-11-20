# Getting Started with Zyvor Fabric

This section covers everything you need to go from a fresh Linux server to a running VM management platform.

---

## Reading Order

Follow these guides in sequence for the best experience:

### 1. [Installation](01-Installation.md)

Install Zyvor Fabric on your Linux system. Covers system requirements, package installation for Fedora and Ubuntu/Debian, and building from source with the Rust toolchain.

**Time:** 5-15 minutes

### 2. [Quick Start](02-Quick-Start.md)

Create your first virtual machine. Start the daemon, create a VM, access the web UI, use the WebSocket console, and clean up.

**Time:** 5 minutes

### 3. [Configuration](03-Configuration.md)

Understand the configuration file format and all available options. Covers daemon settings, storage paths, network configuration, authentication, CORS, and environment variables.

**Time:** 10-20 minutes (reference material)

### 4. [Web UI](04-Web-UI.md)

Access and use the web dashboard. Log in, manage VMs through the browser, and connect to VM consoles.

**Time:** 5 minutes

---

## Prerequisites

Before starting, ensure you have:

- A Linux system with **systemd 256 or later** (Fedora 41+, Ubuntu 24.10+, or equivalent)
- **Root access** (or sudo privileges) for daemon installation
- **QEMU/KVM** installed for VM execution
- A VM disk image (qcow2 format recommended) or access to cloud image downloads

---

## Quick Deploy

If you want to skip the detailed guides and get running immediately:

```bash
git clone <repository-url>
cd zyvor-fabric

# One-command deployment (auto-sudo)
./zyvor-fabricd-ctl deploy

# Read the generated admin password
./zyvor-fabricd-ctl password

# Open the web dashboard
open http://127.0.0.1:9095
```

The `deploy` command handles dependency installation, building from source, installing binaries and systemd units, and starting the service.

---

## Next Steps

After completing the getting started guides:

- [API Reference](../api.md) -- integrate with the REST API
- [Networking Guide](../networking.md) -- configure bridges, VLANs, and network policies
- [Storage Guide](../storage.md) -- set up storage pools and volumes
- [Security Guide](../security.md) -- configure RBAC, API keys, and audit logging
- [Product Overview](../PRODUCT_OVERVIEW.md) -- explore the full feature set
