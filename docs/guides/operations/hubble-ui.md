# Hubble UI (external link)

Fabric and FluxVM **do not** implement Cilium-native VM endpoints or an
in-tree Hubble UI. VMs remain FluxVM edge endpoints (`mode = ebpf|cilium`
coexistence); Cilium owns cluster workloads.

When you already run Hubble elsewhere, point Fabric at it:

```toml
[network]
bridge = "vmbr0"
hubble_ui_url = "https://hubble.example.com"
```

When set, the Edge Dataplane page shows **Open Hubble** (new tab). This is a
convenience link only — no map sync, no CEP ownership.

For VM-edge observe (FluxVM-native), use Edge Dataplane → **Packet flow**
(Colorful / Normal) or `GET /api/dataplane/hubble/flows`.

```bash
fabricctl dataplane hubble --style color
fabricctl dataplane hubble --style plain
fabricctl dataplane hubble --style json   # or: -o json
```

This is Hubble-*lite* from FluxVM (guest → tap → tc/eBPF → uplink → peer), not
Cilium Hubble gRPC. Pair with the FluxVM `feat/hubble-packet-flow` PR so hops
are populated; otherwise Fabric still draws a path from sampled per-VM flows.
