# Web UI Guide

Zyvor Fabric serves an Apple-style hybrid web UI from the daemon: public marketing pages plus a light authenticated console under `/app`.

---

## Surfaces

| Surface | Routes | Notes |
|---------|--------|--------|
| Marketing | `/`, `/product`, `/platform`, `/security` | Public product storytelling |
| Sign-in | `/sign-in` | `/login` redirects here |
| Console | `/app/*` | Dashboard, VMs, network, storage, ops, … |

Legacy bookmarks such as `/vms` redirect to `/app/vms`.

---

## Accessing the UI

### Default URL

```
http://127.0.0.1:9095
```

The UI is served by `zyvor-fabricd` on the same port as the API. No separate web server is required.

### Remote Access

```toml
[daemon]
listen = "0.0.0.0:9095"
cors_origins = ["http://your-server-ip:9095"]
```

For HTTPS, generate a TLS certificate with `./zyvor-fabricd-ctl tls`.

---

## Sign-in

1. Open `/sign-in` (or click **Sign in** on the marketing site).
2. Username `admin`; password from `./zyvor-fabricd-ctl password` or `/var/lib/zyvor-fabricd/.admin_password`.
3. After login you land on `/app`.

JWT sessions last `auth.token_expiration_hours` (default 24h). Expired sessions redirect to `/sign-in`.

| Role | Capabilities |
|------|----------------|
| **Admin** | Full access |
| **User** | Create/start/stop VMs; view metrics |
| **Viewer** | Read-only |

---

## Console overview

- **Left nav** — Core, Infra, Ops, Observe, Secure, Tools, More
- **Top bar** — brand, search / ⌘K, live WebSocket badge, account, sign out
- **Dashboard** (`/app`) — fleet health, capabilities, getting-started when empty
- **VMs** (`/app/vms`) — list, detail tabs, console/VNC at `/app/vms/:name/console`
- **Create** (`/app/create`) — multi-step wizard

### Command palette

`Ctrl+K` / `Cmd+K` — fuzzy search pages and VMs.

### Sequence shortcuts

`g` then `d` / `v` / `n` / `s` / `c` / `l` / `b` / `i` / `e` jumps within `/app`. Press `?` for help.

---

## Visual system

Light Apple-style console using **SF Pro / system UI** fonts and high-contrast tokens (`#1d1d1f` / `#333336` / `#6e6e73` on `#f5f5f7`). Themes `dark` / `steel` / `aurora` are removed. See [ux.md](../ux.md).

---

## Other interfaces

Human UI is **web only**. Automation remains via:

- CLI — `fabricctl`
- Kubernetes operator
- Terraform provider

The former terminal UI (`zyvorctl-tui`) has been removed.

---

## Related

- [User page index](../user/PAGE_INDEX.md)
- [UX conventions](../ux.md)
- [Web UI summary](../web-ui.md)
- [API reference](../api.md)
