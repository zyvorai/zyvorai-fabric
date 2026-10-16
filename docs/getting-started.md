# Getting started

Install, start the daemon, pick a deploy path and verify it. Back to the [README](../README.md).

## Quick start

```bash
git clone https://github.com/zyvorai/fabric.git && cd fabric
make build && sudo make install

# Start the daemon (systemd optional)
sudo zyvor-fabricd
# or: sudo systemctl enable --now zyvor-fabricd

# CLI
fabricctl list
fabricctl create web-01 --image fedora-41 --cpus 2 --memory 4096 --tenant acme

# Web UI → https://localhost:9095  (console at /app; Create VM has optional Tenant)
```

| Goal | Path |
|------|------|
| **Ship stack (easiest)** | `./scripts/ship USER@HOST` |
| Local eval with containers | `make docker-up` → [docs/DOCKER.md](DOCKER.md) |
| Bare-metal remote host | `./scripts/deploy remote USER@HOST` |
| **Kubernetes (k3s lab / Helm)** | [`./scripts/deploy k8s USER@HOST`](deploy.md#run-on-kubernetes) → [docs/KUBERNETES.md](KUBERNETES.md) |
| **AI inference (Beta)** | [Tutorial 15](tutorials/15-ai-workloads.md) · [docs/ai-workloads.md](ai-workloads.md) · console `/app/ai` |
| **Keep** (open agent workstation) | [Tutorial 16](tutorials/16-merion-workstation.md) · [Tutorial 17](tutorials/17-merion-pdf-brief.md) · [docs/merion/MERION.md](merion/MERION.md) · `./scripts/merion-live-lab.sh` · console `/keep` · [Pages](https://zyvorai.github.io/fabric/keep) · `./scripts/merionctl` |
| Declarative VMs | `fabricctl apply -f config.yaml` |
| Terraform | [terraform-provider/](../terraform-provider/) |
| K8s operator (CRDs → API) | [operator/](../operator/) |
| Ansible | [ansible/](../ansible/) |
| Dev on a laptop | [QUICKSTART.md](../QUICKSTART.md) |

Default ports: **9095** (API + UI), **7788** (FluxVM on localhost).

Verify after start:

```bash
curl -sf http://127.0.0.1:9095/health
curl -sf http://127.0.0.1:9095/readyz | jq '{ok, store, fluxvm_ok: .fluxvm.ok}'
curl -sf http://127.0.0.1:7788/readyz | jq .
# Multi-tenant: fabricctl create … --tenant acme; JWT tenant claim scopes list/get/mutate
# When FluxVM auth is on: set driver.fluxvm_token in zyvor-fabricd.toml
```
