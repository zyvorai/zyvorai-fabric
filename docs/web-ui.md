# Web UI

Apple-hybrid React UI for Zyvor Fabric: public marketing pages plus a light authenticated console under `/app`, wired to the existing daemon API.

---

## Surfaces

| Surface | Routes |
|---------|--------|
| Marketing | `/`, `/product`, `/platform`, `/security` |
| Sign-in | `/sign-in` (legacy `/login` redirects here) — two-step: username → **Continue** → password; design tokens in both themes, animated aurora and grid backdrop (static under reduced-motion); no default password is printed |
| Console | `/app/*` (dashboard, VMs, network, storage, ops, …) |
| Container Groups | `/app/container-groups` — FluxVM Secure Containers workloads; see [container-groups.md](container-groups.md) |
| Agents / Sessions | `/app/agents`, `/app/sessions` — agent-runtime proxy (503 unless `[agent_runtime]` configured) |

---

## Features

- Real-time VM status updates via WebSocket
- Interactive terminal console (xterm.js) and graphical VNC console (noVNC)
- Live metrics graphs
- Cilium-style network security management with 9 tabs
- **VM edge dataplane (Network Fabric schema v4)** — VM detail → **Dataplane** tab
  (Status / Policy / Effective / Stats / Flows), **Infrastructure → Edge Dataplane**
  (`/app/edge-dataplane`), plus Dashboard **VM dataplane** capability card; drop-reasons /
  pod-policy APIs and `pod_ingress_*` / `pod_policy` fields when FluxVM reports them; see
  [fluxvm-dataplane.md](guides/vm-drivers/fluxvm-dataplane.md)
- **Service Fabric v6 (BPF schema 4 / gen 6)** — Edge Dataplane → **Services** tab for Maglev VIP
  LB (drain/health/ads/GC/flows/EDT/HA deltas/policy); see [ebpf-service-fabric.md](ebpf-service-fabric.md)
- Command palette (`Ctrl+K` / `Cmd+K`) and `g` sequence shortcuts
- Toast notifications and structured API error banners
- Light Apple visual system (SF Pro / system UI font; no dark/steel/aurora themes)
- High-contrast tokens aligned with apple.com (`#1d1d1f` / `#333336` / `#6e6e73` on `#f5f5f7`)
- Responsive console with collapsible mobile nav

Human UI is **web only**. CLI (`fabricctl`), Kubernetes operator, and Terraform remain. Terminal UI (`zyvorctl-tui`) removed.

### Dataplane console (detail)

| Location | What |
|----------|------|
| `/app` | Capability card **VM dataplane** — Live when FluxVM eBPF is attached (`schema=4`) |
| `/app/vms/:name` → **Dataplane** | Status, policy (v4 fields), **Effective**, stats, flows |
| `/app/edge-dataplane` | Health · **Services** (Maglev v6) · Groups · CNP · Identities · Observe · Ipcache · **Refresh DNS** (best-effort) |
| `/app/vms/:name` → **Network** | Teaser button **Open Dataplane** |

Policy ports must be entered as `tcp/443` / `udp/53`. Bridged (`network_tap`)
VMs are required for attach; user-mode NAT VMs will not show `attached=true`.

---

## Tech Stack

| Library | Purpose |
|---------|---------|
| React 19 | UI framework |
| TypeScript | Type safety |
| Vite | Build tooling |
| Tailwind CSS 4 | Styling |
| React Router 7 | Navigation |
| Recharts | Metrics graphs |
| Lucide React | Icons |
| xterm.js | Terminal console |
| noVNC | VNC display |

---

## Development

```bash
cd web
npm install
npm run dev          # Vite on :5173 (or :3000), proxies API to :9095
npm run build        # → web/dist (served by zyvor-fabricd)
```

### Preview on macOS

`zyvor-fabricd` is Linux-only. For UI work on Darwin:

```bash
python3 scripts/mock-api-preview.py   # mock API on :9095
cd web && npm run dev -- --host 127.0.0.1 --port 3000
# Sign in: admin / any password (e.g. preview)
```

See also [ux.md](ux.md) for conventions and [user/PAGE_INDEX.md](user/PAGE_INDEX.md) for the full route catalog.
