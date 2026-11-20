# Fabric Doctor: production-readiness gate

Fabric Doctor is designed to be used at four lifecycle points:

1. **Before installation** — `fabric-doctor check --skip-service-ping`
2. **After installation** — `fabric-doctor check --strict-services`
3. **Before admitting a new host** — store the JSON report in the provisioning pipeline and reject any report with `summary.failed > 0`
4. **During support incidents** — `fabric-doctor bundle`, with logs/config opt-in

## Recommended admission policy

For a production VM host:

- all `fail` results block admission
- `compute.qemu` may remain `warn` when FluxVM is configured for a non-QEMU backend
- `host.time_sync` should be treated as blocking by the provisioning platform even though Doctor reports it as a warning
- use at least `--min-free-gib 50` for a small node and set a higher site-specific threshold for image-heavy nodes
- use `--strict-services` only after Fabric and FluxVM are expected to be running

Service checks (after install): Fabric `GET /health` + `GET /readyz`, FluxVM TCP
and `GET /readyz` (`--fabric-ready-url` / `--fluxvm-ready-url` override defaults).

## Kubernetes / privileged DaemonSet use

When Fabric runs as a privileged host-network DaemonSet, execute Doctor with the same host visibility if you want meaningful `/dev/kvm`, TUN, cgroup and kernel-module checks. A restricted pod will correctly report those host capabilities as unavailable.

## Future integration

The JSON schema is intentionally stable so the same engine can later back:

- `fabricctl doctor`
- a Fabric `/api/system/readiness` endpoint
- the web console's **Host Readiness** page
- a Kubernetes admission/preflight Job
- bare-metal bootstrap gates in HyperCluster/Machina
