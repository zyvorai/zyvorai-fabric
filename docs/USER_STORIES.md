# zyvor-fabric User Stories

**Product:** private cloud control plane with VM lifecycle handled by FluxVM (no systemd dependency)

Cross-reference: [Documentation index](README.md) · [Main README](../README.md)

## Personas

| Persona | Name | Focus |
|---------|------|-------|
| Private Cloud Admin | Alex | VM lifecycle via FluxVM |
| Platform Engineer | Morgan | K8s operator and Terraform |
| Developer | Jordan | CLI/web console/API for VM operations |

---

### Story 1 — Create VM via CLI

**As Alex** (Private Cloud Admin), I want full Fabric VM lifecycle from fabricctl, **so that** I deliver reliable outcomes.

| Criterion | Notes |
|-----------|-------|
| Core capability | 780+ REST endpoints |

---

### Story 2 — Web dashboard ops

**As Jordan** (Developer), I want manage fleet from react ui with vnc, **so that** I deliver reliable outcomes.

| Criterion | Notes |
|-----------|-------|
| Core capability | web/, noVNC proxy |

---

### Story 3 — HA clustering

**As Morgan** (Platform Engineer), I want multi-node fabric with live migration, **so that** I deliver reliable outcomes.

| Criterion | Notes |
|-----------|-------|
| Core capability | HA crates, clustering |

---

### Story 4 — Terraform provider

**As Morgan** (Platform Engineer), I want declarative infra as code, **so that** I deliver reliable outcomes.

| Criterion | Notes |
|-----------|-------|
| Core capability | terraform-provider/ |

---

### Story 5 — GPU passthrough

**As Alex** (Private Cloud Admin), I want assign gpu to vm workloads, **so that** I deliver reliable outcomes.

| Criterion | Notes |
|-----------|-------|
| Core capability | GPU support in API |

---

## Validation

Map each story to smoke tests, CI jobs, or manual lab steps before marking production-ready.
