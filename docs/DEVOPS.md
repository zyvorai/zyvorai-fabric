# Using Zyvor Fabric in DevOps

Fabric is the **control plane**. FluxVM is the **VM engine**. Pipelines should treat them as one stack.

```
Git / CI  →  zyvor-fabricd :9095  →  fluxvm :7788  →  KVM guests
                 /health /readyz         /healthz /readyz
```

## Probe contract

| Service | Liveness | Readiness |
|---------|----------|-----------|
| Fabric | `GET /health` | `GET /readyz` (`ok`, `store`, `fluxvm`) |
| FluxVM | `GET /healthz` | `GET /readyz` (`ok`, optional `kvm` / `dataplane`) |

`/readyz` is what load balancers and `kubectl` probes must use. Fabric is not ready if FluxVM `/readyz` is 503.

Canonical JSON: [contracts/fabric-fluxvm-readyz.json](contracts/fabric-fluxvm-readyz.json).

## Environment promotion

1. **PR** — contract unit tests + Catch-up coverage job + `scripts/test-devops-gate.sh` + proven-infra suites.
2. **Lab** — `./scripts/ship sus@HOST` (FluxVM + Fabric + readiness). Full lab pack: `scripts/test-lab-verify.sh`. Optional catch-up verify: `scripts/feat-catchup-verify.sh`.
3. **Prod** — snapshot (`scripts/upgrade-rollback.sh snapshot`) → `FABRIC_ADMIN_PASSWORD=… ./scripts/ship sus@HOST --prod` → keep snapshot.

A push to `main` also deploys the lab host from GitHub Actions
([`.github/workflows/lab-deploy.yml`](../.github/workflows/lab-deploy.yml)).
The job runs `./scripts/deploy-remote.sh 80.79.5.173 sus --quick --e2e --verify-apis`
and authenticates with the `LAB_DEPLOY_KEY` repository secret (an ed25519 key
for `sus`, not a personal key). You can start the same job with
`workflow_dispatch`. It stops `zyvor-fabricd` for the remote rebuild, so it
does not run on pull requests.

Pair FluxVM upgrades with [fluxvm `scripts/upgrade-snapshot.sh`](https://github.com/zyvorai/fluxvm) on the same change window.

## Lab HTTPS

Lab `zyvor-fabricd` usually serves **HTTPS with a self-signed cert**.
`scripts/devops-gate.sh` uses `curl -k` and, when `FABRIC_URL` is unset, probes
`https://127.0.0.1:9095` then `http://127.0.0.1:9095`.

`fabricctl` does the same for its default server: with no `--server`,
`ZYVOR_FABRIC_URL` or `FABRIC_URL` it uses `https://localhost:9095` when that
port answers a plain-HTTP request the way a TLS listener does, and
`http://localhost:9095` otherwise (the Docker config serves plain HTTP). An
explicit server is used as given.

```bash
# Super-easy stack ship (FluxVM + Fabric + readiness)
./scripts/ship sus@HOST

# Full post-deploy lab gate (stdin closed for nested tools)
./scripts/test-lab-verify.sh

# Production readiness only (read-only; requires real token — no Admin@321 fallback)
FABRIC_URL=https://127.0.0.1:9095 FLUXVM_URL=http://127.0.0.1:7788 \
  FABRIC_TOKEN=… ./scripts/test-production-readiness.sh

# Live probe only
unset FABRIC_URL
FLUXVM_URL=http://127.0.0.1:7788 ./scripts/devops-gate.sh
# or:
ZYVOR_DEVOPS_LIVE=1 ./scripts/test-devops-gate.sh
```

## Security and format scans in CI

- **Trivy** (`.github/workflows/security.yml`) scans the tree for vulnerabilities and secrets at
  HIGH and CRITICAL. [`trivy-secret.yaml`](../trivy-secret.yaml) at the repo root is Trivy's secret
  config; it allows only the fake `ghp_…` tokens in `agent-runtime/src/memory.rs` (the memory
  redaction tests need a token-shaped value). Every other path is still scanned. Add a new allow
  rule there only for a test fixture, with a `path` limited to that file.
- **CodeQL** reports to the repository's code-scanning page. A false positive is dismissed there
  with a written reason (for example the operator-set path in `agent-runtime/src/audit.rs`); a real
  one gets a code fix and a test.
- **Fabric Doctor** (`.github/workflows/fabric-doctor.yml`) runs `gofmt -l ./cmd ./internal` in
  `tools/fabric-doctor`; run `gofmt -w` on the listed files before pushing.

## Source of truth (pick one)

- GitOps: `examples/devops/gitops` + operator
- Terraform: `examples/devops/terraform`
- CLI: `fabricctl apply -f examples/devops/apply-vm.yaml`

Do not mix writers on the same VM name.

## Secrets

- `ZYVOR_FABRICD_ADMIN_PASSWORD`, `ZYVOR_FABRICD_JWT_SECRET`
- CI user token (`FABRIC_TOKEN`)
- `driver.fluxvm_token` when FluxVM auth is on

## Examples

See [examples/devops/README.md](../examples/devops/README.md).
