# Zyvor Fabric — Quick Start Guide

## Prerequisites

- Rust 1.70+ (`rustup`)
- Node.js 18+ and npm
- [FluxVM](https://github.com/zyvorai/fluxvm) running (`fluxvm serve`) for actual VM management — optional for building/running the daemon itself, but needed to create/manage VMs

## Build Everything

```bash
# Clone the repository
git clone https://github.com/zyvorai/fabric.git
cd fabric

# Build backend (Rust)
cd backend
cargo build --release
cd ..

# Build web UI (React)
cd web
npm install
npm run build
cd ..
```

## Run in Development Mode

### Terminal 1: Backend

```bash
cd backend
cargo run --bin zyvor-fabricd
```

### Terminal 2: Web UI (marketing at `/`, console at `/app`)

```bash
cd web && npm install && npm run dev
# → http://127.0.0.1:5173 (proxies /api → :9095)
# Access marketing at / and console at /app after sign-in
```

On macOS the daemon does not build; use the mock API instead:

```bash
python3 scripts/mock-api-preview.py   # :9095
cd web && npm run dev -- --host 127.0.0.1 --port 3000
# Sign in at /sign-in — admin / any password
```

## Try the CLI

```bash
# List VMs
./backend/target/debug/fabricctl list
./backend/target/debug/fabricctl list -o json      # JSON output
./backend/target/debug/fabricctl list -o yaml      # YAML output

# Create a VM
./backend/target/debug/fabricctl create myvm \
  --image=/path/to/image.qcow2 \
  --cpus=2 \
  --memory=2048

# Start a VM
./backend/target/debug/fabricctl start myvm

# Apply config from YAML file
./backend/target/debug/fabricctl apply -f vm.yaml

# Network security
./backend/target/debug/fabricctl policy list
./backend/target/debug/fabricctl firewall list
./backend/target/debug/fabricctl vpn tunnels

# Ceph storage
./backend/target/debug/fabricctl ceph create my-pool --monitors=10.0.0.1 --pool=rbd
./backend/target/debug/fabricctl ceph health my-pool

# Open web UI at http://localhost:9095 (console: /app)
```

## Install System-Wide

```bash
# Run install script
./scripts/install.sh

# Enable and start daemon
sudo systemctl enable --now zyvor-fabricd

# Use CLI
fabricctl list
```

## Remote bare-metal deploy

```bash
./scripts/deploy remote sus@HOST
./scripts/deploy remote sus@HOST --quick
# UI: https://HOST:9095/   password: sudo cat /var/lib/zyvor-fabricd/.admin_password
```

## Run on Kubernetes

Privileged `hostNetwork` DaemonSets (fabricd + FluxVM). Full guide: [docs/KUBERNETES.md](docs/KUBERNETES.md).

```bash
# Lab k3s — build + import + apply on the remote node
./scripts/deploy k8s sus@HOST
# UI: http://HOST:30095/

# Local cluster (images already loaded)
make k8s-deploy

# Helm
helm upgrade --install zyvor-fabric ./charts/zyvor-fabric \
  --namespace zyvor-fabric --create-namespace \
  --set security.adminPassword='...' \
  --set security.jwtSecret="$(openssl rand -base64 32)"
```

Docker / Podman eval: [docs/DOCKER.md](docs/DOCKER.md) (`make docker-up`).

## Access Web UI

Once Zyvor Fabric is running, access:

```
http://localhost:9095
```

## Authentication

When auth is enabled (default), Zyvor Fabric creates an `admin` user on first startup:

```bash
# Read the auto-generated admin password
sudo cat /var/lib/zyvor-fabricd/.admin_password

# Login to get a JWT token
TOKEN=$(curl -s -X POST http://localhost:9095/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username": "admin", "password": "<password-from-file>"}' | jq -r .token)

# Use the token for API calls
curl -H "Authorization: Bearer $TOKEN" http://localhost:9095/api/vms
```

To set a custom admin password, use the `ZYVOR_FABRICD_ADMIN_PASSWORD` environment variable before first startup.

When auth is disabled (`enabled = false` in config), API calls work without a token.

## Use the CLI

`fabricctl` picks its server in this order: `--server`, `ZYVOR_FABRIC_URL`, `FABRIC_URL`, then a
default of `https://localhost:9095` (if that port speaks TLS) or `http://localhost:9095`. It sends
the token from `--token`, `ZYVOR_FABRIC_TOKEN` or `FABRIC_TOKEN`, and accepts a self-signed
certificate for an `https://` server.

```bash
export FABRIC_TOKEN="$TOKEN"     # from the login step above
fabricctl status                 # API, auth token and dataplane
fabricctl list                   # VMs
fabricctl -o json list           # json | yaml | table
fabricctl --server https://fabric.example:9095 list
```

A default install serves **HTTPS with a self-signed certificate**, so the `curl` examples below
need `https://localhost:9095` and `-k` unless you use the Docker config, which serves plain HTTP.

## Test the API

```bash
# With auth disabled:
curl http://localhost:9095/api/vms

# With auth enabled (see Authentication above):
curl -H "Authorization: Bearer $TOKEN" http://localhost:9095/api/vms

# Create a VM
curl -X POST http://localhost:9095/api/vms \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "test-vm",
    "image": "/path/to/image.qcow2",
    "cpus": 2,
    "memory": 2048
  }'

# Start a VM (basic)
curl -X POST http://localhost:9095/api/vms/test-vm/start \
  -H "Authorization: Bearer $TOKEN"

# Start a VM with options (all fields optional)
curl -X POST http://localhost:9095/api/vms/test-vm/start \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "kvm": true,
    "tpm": true,
    "network_tap": true,
    "console": "interactive",
    "credentials": [
      { "id": "passwd.hashed-password.root", "value": "$y$..." }
    ]
  }'
```

## Directory Structure

```
fabric/
├── backend/              # Rust backend (daemon, CLI, drivers, enterprise)
├── web/                  # React web UI (baked into fabricd image)
├── k8s/base/             # Kubernetes manifests (DaemonSets + NodePort)
├── charts/zyvor-fabric/  # Platform Helm chart (fabricd + FluxVM)
├── operator/             # Kubernetes operator (CRDs → Fabric API)
├── terraform-provider/   # Terraform provider
├── docs/                 # Documentation (see docs/KUBERNETES.md)
├── systemd/              # Optional systemd unit files
├── configs/              # Configuration files
├── scripts/              # deploy, deploy-k8s, install, verify
├── monitoring/           # Monitoring configuration
├── sdk/                  # SDK
├── tests/                # Integration tests
└── debian/               # Debian packaging
```

## Next Steps

1. Read [Architecture](docs/architecture.md)
2. Deploy on [Kubernetes](docs/KUBERNETES.md) or [Docker](docs/DOCKER.md)
3. Explore [API Documentation](docs/api.md)
4. Optional: drive Fabric with the [OpenStack CLI tutorial](docs/tutorials/08-openstack-clients.md)
5. Check out the [Web UI](docs/web-ui.md)
6. Review [Security](docs/security.md)
7. Explore [Advanced Features](docs/advanced-features.md)
8. Deploy durable, FluxVM-sandboxed TypeScript agents: [Agent Runtime Quickstart](docs/tutorials/11-agent-runtime-quickstart.md)
9. Run Kubernetes-style Pod groups on FluxVM Secure Containers: [Container Groups](docs/container-groups.md)

## Troubleshooting

### Build fails

```bash
# Update Rust
rustup update

# Clean and rebuild
cd backend && cargo clean && cargo build
```

### Web UI not loading

```bash
# Check if daemon is running
curl http://localhost:9095/health

# Should return: OK
```

### Permission errors

Zyvor Fabric requires root privileges to manage VMs:

```bash
sudo ./backend/target/release/zyvor-fabricd
```
