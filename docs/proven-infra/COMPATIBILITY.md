# Compatibility matrix

Issue #17 — Fabric × FluxVM × GuestKit × Kubernetes versions.

Status values:

- **CI** — exercised on GitHub Actions (`ci.yml`, `fabric-e2e.yml`, `operator.yml`, `agent-runtime.yml`). `lab-deploy.yml` rebuilds the lab host on push to `main`.
- **Lab** — documented lab path (`docs/DOCKER.md`, `docs/KUBERNETES.md`)
- **Unverified** — combination exists in the wild but is not gated here

## Control plane

| Fabric | Workspace | Notes |
|--------|-----------|-------|
| 0.2.1 / `main` | 48+1 (`backup` now a workspace member) | JWT tenant scoping, `/readyz`, OpenStack façade experimental |

## VM engine

| FluxVM | Fabric driver | Status |
|--------|---------------|--------|
| FluxVM `main` / `/readyz` present | `crates/fluxvm-driver` + `driver.fluxvm_url` | **Lab** + e2e when `FLUXVM_URL` is set |
| FluxVM with auth token | `driver.fluxvm_token` | **Lab** |
| No FluxVM | daemon serves API/UI only | **CI** (API audit, unit tests) |

Fabric does not execute VMs itself. Combinations without a reachable FluxVM cannot create/start/stop guests.

## Disk tooling

| GuestKit | Used for | Status |
|----------|----------|--------|
| GuestKit `main` | Offline image customize (NBD) | **Lab** — host `nbd` module required |
| Absent | qcow2/raw as-is | **CI** |

## Kubernetes

| Kubernetes | Fabric deploy | Operator | Status |
|------------|---------------|----------|--------|
| kind (CI) | not required | `operator/` reconcile e2e (mocked API) | **CI** |
| k3s / kubeadm 1.29–1.33 | Helm `charts/zyvor-fabric` hostNetwork DaemonSets, NodePort 30095 | Helm `operator/charts/zyvor-fabricd-operator` | **Lab** |
| OpenShift / managed EKS/GKE/AKS | not packaged | Unverified | **Unverified** |

## Auth / identity

| Provider | Status |
|----------|--------|
| Local JWT + bcrypt users | **CI** |
| OIDC PKCE + JWKS | **CI** unit + documented Keycloak/Entra/Okta |
| SCIM 2.0 | **CI** crate tests |
| OpenStack Keystone façade | **Experimental** — password not bound to Fabric identity |

## Storage backends (control-plane API)

| Backend | Create/attach in unit tests | Live I/O |
|---------|-----------------------------|----------|
| Local filesystem | yes (`backup` crate) | **CI** |
| NFS / LVM / ZFS / Ceph | API + docs | **Lab** |

## How to extend this matrix

Add a row only after a recorded run:

```bash
./scripts/chaos-qualify.sh --report docs/proven-infra/runs/
./benchmarks/harness.py --base-url "$FABRIC_URL" --out benchmarks/baselines/$(date +%F).json
```
