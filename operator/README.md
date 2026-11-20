# Zyvor Fabric Kubernetes Operator

Manage virtual machines as native Kubernetes resources. The operator watches `VirtualMachine` custom resources and reconciles them against the Zyvor Fabric API, handling creation, updates, deletion, and status reporting automatically.

> **Running fabricd itself on Kubernetes?** That is the platform chart / `k8s/base` DaemonSets — see **[docs/KUBERNETES.md](../docs/KUBERNETES.md)**. This operator assumes fabricd is already reachable.

## Features

- Declarative VM management via `kubectl`
- cloud-init, TPM, and VNC support in CRD spec
- Auto-reconciliation -- continuously ensures desired state matches actual state
- Status subresource reflects real-time VM state
- Helm chart with RBAC and resource limits for production deployment
- Leader election for high-availability operator deployments
- Optional `InferenceDeployment` validating webhook (off by default) that calls fabricd `POST /api/ai/admit/{token}` — see [AI Workloads](../docs/ai-workloads.md)

## Quick Start

### Prerequisites

Fabric API must be up (bare metal, Docker, or [Kubernetes platform deploy](../docs/KUBERNETES.md)).

### Install the operator

```bash
# From this repo (chart under operator/charts/)
helm upgrade --install zyvor-fabricd-operator ./operator/charts/zyvor-fabricd-operator \
  --set zyvor-fabricd.url=http://NODE_IP:30095
# or same-node hostNetwork fabricd:
#   --set zyvor-fabricd.url=http://127.0.0.1:9095

# Optional InferenceDeployment admit webhook (off by default):
#   --set admissionWebhook.enabled=true \
#   --set admissionWebhook.token="$FLUXVM_AI_ADMIT_TOKEN" \
#   --set admissionWebhook.fabricdUrl=https://NODE_IP:9095 \
#   --set admissionWebhook.caBundle="$(base64 -w0 /etc/zyvor-fabricd/tls/server.crt)"
# Lab k3s can install CRDs + webhook with replicaCount=0 so only admission
# runs against fabricd on the host (no operator pod required for that check).
```

### Create a VM

Define a `VirtualMachine` resource (see [examples/vm-example.yaml](examples/vm-example.yaml)):

```yaml
apiVersion: zyvor-fabricd.io/v1alpha1
kind: VirtualMachine
metadata:
  name: my-vm
spec:
  image: /var/lib/zyvor-fabricd/images/ubuntu-22.04.qcow2
  cpus: 2
  memory: 2048
  cloudInit:
    userData: |
      #cloud-config
      packages:
        - qemu-guest-agent
      runcmd:
        - systemctl start qemu-guest-agent
  tpm:
    enabled: true
    version: "2.0"
  vnc:
    enabled: true
```

Apply it:

```bash
kubectl apply -f vm.yaml
```

### Manage VMs with kubectl

```bash
kubectl get vm                  # List all VMs
kubectl describe vm my-vm       # Detailed status and events
kubectl delete vm my-vm         # Delete a VM
```

## Architecture

```
Kubernetes API Server
        |
        v
  Operator Controller
  (watches VirtualMachine CRs)
        |
        v
  Zyvor Fabric REST API  (bare metal :9095 or K8s NodePort :30095)
        |
        v
  Virtual Machines (via FluxVM)
```

The operator runs a reconciliation loop that:
1. Watches for `VirtualMachine` resource changes
2. Compares desired spec against actual VM state in Zyvor Fabric
3. Creates, updates, or deletes VMs to match the declared state
4. Reports VM status back to the Kubernetes status subresource

## CRD Spec Reference

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `image` | string | Yes | -- | Path to VM disk image |
| `cpus` | integer | No | 2 | Number of vCPUs |
| `memory` | integer | No | 2048 | Memory in MB |
| `cloudInit.userData` | string | No | -- | cloud-init user-data (cloud-config YAML) |
| `tpm.enabled` | boolean | No | false | Enable virtual TPM |
| `tpm.version` | string | No | "2.0" | TPM version (1.2 or 2.0) |
| `vnc.enabled` | boolean | No | false | Enable VNC display |

## Configuration

The operator is configured via environment variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `ZYVOR_FABRICD_URL` | `http://127.0.0.1:9095` | URL of the zyvor-fabricd API (use `http://NODE_IP:30095` for lab K8s NodePort) |
| `ZYVOR_FABRICD_TOKEN` | (empty) | Optional JWT for authenticated API calls |
| `RUST_LOG` | `info` | Log level (trace, debug, info, warn, error) |

## Development

```bash
# Build the operator
cd operator
cargo build --release

# Run locally (requires kubeconfig and Zyvor Fabric access)
cargo run
```
