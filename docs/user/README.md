# Zyvor Fabric — User Documentation

Private cloud control plane with no systemd dependency — vCenter-style VM ops, DRS, FT, replication, and distributed storage.

| You want to… | Open |
|--------------|------|
| Install and log in | [Getting Started](getting-started.md) |
| Learn the shell | [Using the Dashboard](using-the-dashboard.md) |
| Follow a page, step by step | [Page-by-page guides](pages/README.md) |
| Look up any screen | [Complete page index](PAGE_INDEX.md) |
| Deploy, auth, ports | [Admin basics](admin-basics.md) |
| Multi-page jobs | [Common workflows](workflows.md) |
| Capability map | [Feature Guide](../zyvor-fabric-user-feature-guide.md) |

## Printable PDFs

```bash
node scripts/user-docs/build-user-pdfs.mjs
```

Output lands in [`pdf/`](pdf/):

| PDF | Contents |
|-----|----------|
| `Zyvor-Fabric-User-README.pdf` | This overview |
| `Zyvor-Fabric-Getting-Started.pdf` | Access, basics, workflows |
| `Zyvor-Fabric-Page-by-Page.pdf` | Complete page manual |
| `Zyvor-Fabric-Admin-Basics.pdf` | Deploy, auth, ports |

## Product at a glance

Private cloud control plane with no systemd dependency — vCenter-style VM ops, DRS, FT, replication, and distributed storage.

## Support surfaces (quick map)

Public marketing: `/`, `/product`, `/platform`, `/security`. Sign in: `/sign-in`. Ops console: `/app/*`.


| Need | Path |
|------|------|
| Dashboard | `/app` |
| VMs | `/app/vms` |
| VM Dataplane (eBPF edge) | `/app/vms/:name` → **Dataplane** — [guide](pages/infrastructure/dataplane.md) |
| Edge Dataplane (cluster) | `/app/edge-dataplane` — [guide](pages/infrastructure/edge-dataplane.md) |
| DRS / FT | `/app/drs`, `/app/fault-tolerance` |
| Site Recovery | `/app/site-recovery` |
| Settings | `/app/settings` |

---

*ZyvorAI Labs · [zyvor.dev](https://zyvor.dev) · Zyvor Fabric*
