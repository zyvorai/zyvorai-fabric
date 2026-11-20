# Advanced Features

## WebSocket Console

Real-time interactive terminal access to any running VM via WebSocket.

**Endpoint:** `ws://localhost:9095/ws/console/:vmname`

The server authenticates the WebSocket upgrade, attaches to the VM's serial console, and relays bidirectional I/O between the client and the VM.

### Browser (xterm.js)

```typescript
const ws = new WebSocket(`ws://localhost:9095/ws/console/myvm?token=${token}`)
term.onData(data => ws.send(data))
ws.onmessage = event => term.write(event.data)
```

### Web UI

1. Navigate to VM details
2. Click **Console**
3. Interactive terminal opens in the browser

---

## VNC / noVNC Integration

Graphical console access via VNC, proxied over WebSocket for browser-based display.

**Endpoint:** `ws://localhost:9095/ws/vnc/:vmname`

```
Browser (noVNC over WebSocket) <-> vnc-proxy <-> VNC Server (TCP 5900+)
```

The proxy translates between WebSocket (browser) and raw TCP (VNC server), avoiding direct VNC port exposure and enabling TLS termination at the daemon level.

Each VM gets a unique VNC display number on ports 5900+.

### Web UI

1. Navigate to VM console
2. Click **VNC** tab
3. Graphical display appears via noVNC

---

## cloud-init

Automated VM initialization with users, packages, network configuration, and custom scripts.

### API

```
POST /api/vms/:name/cloud-init
```

```json
{
  "instance_id": "vm1",
  "hostname": "vm1",
  "user_data": "#cloud-config\n...",
  "network_config": "..."
}
```

### Example User Data

```yaml
#cloud-config
users:
  - name: ubuntu
    sudo: ALL=(ALL) NOPASSWD:ALL
    shell: /bin/bash
    ssh_authorized_keys:
      - ssh-rsa AAAA...
packages:
  - qemu-guest-agent
  - docker.io
runcmd:
  - systemctl start qemu-guest-agent
write_files:
  - path: /etc/motd
    content: "Welcome to vm1"
    permissions: "0644"
```

### How It Works

1. `POST /api/vms/:name/cloud-init` parses `user_data` and persists `hostname`, `ssh_authorized_keys`, `packages`, `runcmd`, and `write_files` onto the VM record.
2. On the VM's next (re)start, those fields are passed to FluxVM as part of its own `CloudInitSpec`, which is what makes FluxVM attach a cloud-init seed disk (NoCloud datasource) to the VM.
3. cloud-init inside the guest reads that seed on first boot.
4. Because attaching the seed is what brings up cloud-init's own default network config, this is also what makes the guest's DHCP client come up — a VM with no cloud-init config at all gets no seed and no automatic networking.

---

## TPM / vTPM

Virtual Trusted Platform Module for secure boot, disk encryption, and remote attestation.

### Requirements

```bash
sudo dnf install swtpm swtpm-tools    # Fedora/RHEL
sudo apt install swtpm swtpm-tools    # Debian/Ubuntu
```

### Capabilities

- TPM 1.2 and 2.0 support (`--tpm=yes`)
- Configurable TPM state persistence (`--tpm-state=auto|off|/path`)
- Automatic lifecycle tied to VM lifecycle
- EK (Endorsement Key) and platform certificates
- Per-VM isolated TPM instances with persistent state
- Compatible with Windows BitLocker and Linux LUKS

### Usage

```rust
let tpm_manager = TPMManager::new("/var/lib/zyvor-fabricd/tpm")?;
let tpm_dir = tpm_manager.create_vtpm(&config).await?;
let pid = tpm_manager.start_swtpm("myvm", TPMVersion::TPM20).await?;
```

### QEMU Integration

```bash
-chardev socket,id=chrtpm,path=/var/lib/zyvor-fabricd/tpm/myvm/swtpm-sock
-tpmdev emulator,id=tpm0,chardev=chrtpm
-device tpm-tis,tpmdev=tpm0
```

---

## Kubernetes Operator

Manage VMs as native Kubernetes resources.

### CRD

```yaml
apiVersion: Zyvor Fabric.io/v1alpha1
kind: VirtualMachine
metadata:
  name: ubuntu-vm
spec:
  image: /path/to/image.qcow2
  cpus: 4
  memory: 4096
  cloudInit:
    userData: |
      #cloud-config
      ...
  tpm:
    enabled: true
  vnc:
    enabled: true
```

### Installation

```bash
kubectl apply -f operator/crd.yaml
helm install zyvor-fabricd-operator operator/charts/zyvor-fabricd-operator
```

### Usage

```bash
kubectl apply -f vm-example.yaml
kubectl get vm
kubectl describe vm ubuntu-vm
kubectl delete vm ubuntu-vm
```

### Architecture

```
Kubernetes API --> Controller (watches VirtualMachine CRs) --> Zyvor Fabric REST API --> VMs
```

The operator continuously reconciles desired state with actual VM state, handling creation, updates, deletion, and status reporting.

See [operator/README.md](../operator/README.md) for full documentation.

---

## Terraform Provider

Declarative VM provisioning via HashiCorp Terraform.

```hcl
provider "Zyvor Fabric" {
  url   = "http://localhost:9095"
  token = var.zyvor_fabricd_token
}

resource "zyvor_fabric_vm" "web" {
  name   = "web-server"
  image  = "/var/lib/zyvor-fabricd/images/ubuntu-22.04.qcow2"
  cpus   = 4
  memory = 4096

  cloud_init {
    user_data = file("cloud-init.yaml")
  }

  tpm {
    enabled = true
    version = "2.0"
  }
}
```

Features: full CRUD lifecycle, import existing VMs, plan/apply workflow with diffs, cloud-init/TPM/VNC/tags support.

See [terraform-provider/README.md](../terraform-provider/README.md) for full documentation.

---

## Prometheus Metrics

**Endpoint:** `GET /metrics`

### VM Metrics

| Metric | Description |
|--------|-------------|
| `zyvor_fabricd_vms_total` | Total VM count |
| `zyvor_fabricd_vms_running` | Running VM count |
| `zyvor_fabricd_vms_stopped` | Stopped VM count |
| `zyvor_fabricd_vm_starts_total` | Total VM start operations |
| `zyvor_fabricd_vm_stops_total` | Total VM stop operations |
| `zyvor_fabricd_vm_creates_total` | Total VM create operations |
| `zyvor_fabricd_vm_deletes_total` | Total VM delete operations |

These seven are the full metric set the daemon exports today. Per-VM CPU/memory/disk/network
utilization and API request/latency metrics are not yet implemented as Prometheus series -- use
`GET /api/vms/{name}/metrics` for live per-VM resource figures in the meantime.

### Prometheus Configuration

```yaml
scrape_configs:
  - job_name: Zyvor Fabric
    static_configs:
      - targets: ['localhost:9095']
    metrics_path: /metrics
    scrape_interval: 15s
```

### Grafana Dashboard

Import the pre-built dashboard from `monitoring/grafana-dashboard.json` for four panels: Total VMs,
Running VMs, VM States, and VM Operations (start/stop/create/delete rates) -- the same metrics listed
above. Per-VM resource graphs, network/disk I/O, and API latency panels aren't available yet since the
underlying metrics don't exist (see the note above).
