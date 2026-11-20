# Deploy

Four first-class ways to run Fabric: bare metal, Docker/Podman, Kubernetes and the operator. Back to the [README](../README.md).

## Deploy

Four first-class ways to run Fabric. Pick one:

```text
┌─────────────────┬──────────────────┬──────────────────┬─────────────────┐
│  Bare metal     │  Docker/Podman   │  Kubernetes      │  Operator only  │
│  systemd/binary │  compose         │  DaemonSets      │  CRDs → API     │
├─────────────────┼──────────────────┼──────────────────┼─────────────────┤
│  Production     │  Local eval      │  Lab k3s /       │  GitOps VMs     │
│  hosts          │                  │  in-cluster CP   │  against fabricd│
└─────────────────┴──────────────────┴──────────────────┴─────────────────┘
```

### Bare metal (systemd) — easiest path

Ship **FluxVM + Fabric** in one command (from the Fabric repo, with sibling `../fluxvm`):

```bash
./scripts/ship sus@HOST              # lab quick redeploy + readiness
./scripts/ship sus@HOST --full       # first install (deps + firewall)
FABRIC_ADMIN_PASSWORD='…' ./scripts/ship sus@HOST --prod
```

Same from FluxVM: `./scripts/ship sus@HOST` (execs sibling Fabric `scripts/ship`).

Advanced (Fabric only):

```bash
./scripts/deploy remote sus@HOST
./scripts/deploy remote sus@HOST --quick    # skip OS deps
./scripts/deploy check sus@HOST
```

Installs `zyvor-fabricd` + web UI, opens `0.0.0.0:9095` (HTTPS, self-signed by default). Admin password is generated on deploy unless you set `FABRIC_ADMIN_PASSWORD` / `ZYVOR_FABRICD_ADMIN_PASSWORD`, or `FABRIC_LAB_DEFAULTS=1` for convenient lab default `Admin@321`. Retrieve: `sudo cat /var/lib/zyvor-fabricd/.admin_password`. Reseed with `FORCE_ADMIN_RESET=1 ./scripts/deploy remote USER@HOST --quick`.

### Docker / Podman

```bash
./scripts/build-container-images.sh   # needs ../FluxVM + ../guestkit
make docker-up                        # hostNetwork + /dev/kvm
# → http://localhost:9095   admin / eval-admin-only
```

See [docs/DOCKER.md](DOCKER.md) for host prerequisites (`nbd`, KVM, rootful engine, cgroup v2).

### Run on Kubernetes

Fabric on Kubernetes uses the same **lab packaging pattern as Ragnarok** (manifests, Helm, remote `k3s ctr import`), but workloads are **privileged `hostNetwork` DaemonSets** — required for nftables, KVM, and FluxVM on `127.0.0.1:7788` (same model as compose).

> Full guide: **[docs/KUBERNETES.md](KUBERNETES.md)**

```bash
# First time: build images on the node, import into k3s, apply manifests
./scripts/deploy k8s sus@HOST

# Later: re-apply + rollout only
./scripts/deploy k8s sus@HOST --quick

# Remove
./scripts/deploy k8s sus@HOST --uninstall
```

| Surface | Port |
|---------|------|
| UI + API (NodePort) | **30095** |
| UI + API (hostNetwork) | **9095** |
| FluxVM | **7788** (node-local) |

Open `http://HOST:30095/` after deploy. **Login:** `admin` + password from Secret `zyvor-fabric-secrets` (generated unless `FABRIC_ADMIN_PASSWORD` or `FABRIC_LAB_DEFAULTS=1`). Retrieve: `kubectl -n zyvor-fabric get secret zyvor-fabric-secrets -o jsonpath='{.data.admin-password}' | base64 -d; echo`.

**Local kubectl / Helm:**

```bash
# Manifests (images must be visible to the cluster)
make k8s-deploy
# or: BUILD_IMAGES=true ./scripts/deploy-k8s.sh

# Helm
helm upgrade --install zyvor-fabric ./charts/zyvor-fabric \
  --namespace zyvor-fabric --create-namespace \
  --set security.adminPassword='...' \
  --set security.jwtSecret="$(openssl rand -base64 32)"
```

**Platform chart vs. operator:**

| Piece | What it does |
|-------|----------------|
| **`charts/zyvor-fabric`** / `k8s/base/` | Runs **fabricd + FluxVM** in the cluster |
| **`operator/charts/zyvor-fabricd-operator`** | Watches `VirtualMachine` CRs and calls an **already-running** fabricd API |

Point the operator at NodePort or the node IP (`ZYVOR_FABRICD_URL=http://NODE_IP:30095`). Do not expect ClusterIP DNS to replace `hostNetwork` across nodes.

**Requirements (K8s):** node with `/dev/kvm` · namespace PSS **privileged** (cannot run restricted) · rootful `podman` or `docker` on the build host for image builds · optional sibling checkouts `../FluxVM` and `../guestkit` for the FluxVM image.
