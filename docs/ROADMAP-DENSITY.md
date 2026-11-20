# Roadmap (Fabric): Cilium / Hubble / density

See FluxVM [ROADMAP-DENSITY.md](https://github.com/zyvorai/fluxvm/blob/main/docs/ROADMAP-DENSITY.md)
for the shared plan. FluxVM Phase **2b** (agent CEP identity) and **3c**
(`FLUXKVM1` KVM memory snapshots) are **Done** on FluxVM.

## Fabric surface

| Track | Fabric status |
|-------|----------------|
| Hubble UI link + Edge Dataplane packet flow | **Done** (`network.hubble_ui_url`, `/api/dataplane/hubble/flows`) |
| CEP-*shaped* endpoints + `identity_source` | **Done** — `GET /api/dataplane/endpoints` → Edge Dataplane **Endpoints** tab |
| MicroVM Prometheus histograms | **Done** — proxy `GET /api/dataplane/microvm-metrics` + scrape job on `:9108` |
| Concurrent density / FLUXKVM1 | **FluxVM lab** — `scripts/bench-density.sh`, `test-kvm-snapshot-smoke.sh`; not QMP Snapshot Manager |

## Notes

- Fabric never writes Cilium private maps. Agent enrich is read-only via FluxVM.
- QMP Disk/Full snapshots stay on classic VMs; in-tree KVM `FLUXKVM1` is FluxVM-only.
- Density numbers: run FluxVM benches on the lab host; Fabric `benchmarks/` is API latency only.

Docs: [hubble-ui.md](guides/operations/hubble-ui.md) ·
[fluxvm-dataplane.md](guides/vm-drivers/fluxvm-dataplane.md).
