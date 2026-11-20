# Admin Basics (Zyvor Fabric)

Operator reference for ports, auth, remote deploy, TLS, and the FluxVM dependency. For the first-hour UI path, see [Getting Started](getting-started.md).

## Ports / access

| Port | Service |
|------|---------|
| **9095** | `zyvor-fabricd` — API + production web UI (same origin) |
| **7788** | FluxVM (node-local VM engine; Fabric proxies it) |
| **5173** | Vite UI (dev only) |

Web routes: marketing `/`, `/product`, `/platform`, `/security`; sign-in `/sign-in`; console `/app/*`.

Open the UI at `https://<host>:9095` or `http://127.0.0.1:9095` on the host. Never publish lab IPs in docs or tickets — use `<host>`.

Health check:

```bash
curl -sf http://127.0.0.1:9095/readyz | jq '{ok, store, fluxvm_ok: .fluxvm.ok}'
```

## Auth and admin password

JWT bearer auth (local admin by default). Optional LDAP/OIDC/PAM for system users.

Sign in at `https://<host>:9095/sign-in` with username `admin`, then open `/app`.

### JWT secret and admin password

Both are auto-generated (cryptographically random) on first start if unset, and persisted so they survive restarts:

| Env var | Default when unset | Persisted at |
|---------|---------------------|--------------|
| `ZYVOR_FABRICD_JWT_SECRET` | Random, once | `/var/lib/zyvor-fabricd/.jwt_secret` (mode 0600) |
| `ZYVOR_FABRICD_ADMIN_PASSWORD` | Random, once — never defaults to `admin` | `/var/lib/zyvor-fabricd/.admin_password` |

Retrieve or manage the password with the ctl:

```bash
./zyvor-fabricd-ctl password          # show
sudo cat /var/lib/zyvor-fabricd/.admin_password
./zyvor-fabricd-ctl password --lab-reset [PASSWORD]   # reseed (lab only)
```

Set `ZYVOR_FABRICD_ADMIN_PASSWORD` (or `FABRIC_ADMIN_PASSWORD`) **before first start** if you want a known value from day one — once persisted, the file wins even if the env var is later unset.

## Deploy (local and remote)

From a Fabric checkout:

| Target | Command |
|--------|---------|
| Local full deploy | `./zyvor-fabricd-ctl deploy` (deps → build → install → start) |
| Bare-metal remote | `./scripts/deploy remote USER@HOST` |
| Remote, skip OS deps | `./scripts/deploy remote USER@HOST --quick` |
| Kubernetes lab | `./scripts/deploy k8s USER@HOST` |

Remote bare-metal install opens `0.0.0.0:9095` (HTTPS with a self-signed cert by default). Admin password is generated on deploy unless you set `FABRIC_ADMIN_PASSWORD` / `ZYVOR_FABRICD_ADMIN_PASSWORD`, or `FABRIC_LAB_DEFAULTS=1` for a convenient lab default. Force reseed: `FORCE_ADMIN_RESET=1 ./scripts/deploy remote USER@HOST --quick`.

Useful ctl commands after install: `status`, `logs`, `verify`, `doctor`, `restart`.

## TLS

```bash
./zyvor-fabricd-ctl tls    # generate self-signed server cert (auto-sudo)
```

Enable the paths the ctl prints in `/etc/zyvor-fabricd/zyvor-fabricd.toml` (typically under `/etc/zyvor-fabricd/tls/`). Browsers will warn on self-signed certs in lab — replace with a real cert for production. For local HTTP-only testing, `http://127.0.0.1:9095` is fine when TLS is off.

## FluxVM dependency

Fabric does **not** run QEMU itself. It orchestrates VMs through a local [FluxVM](https://github.com/zyvorai/fluxvm) instance on `127.0.0.1:7788` (lifecycle, disks, console/VNC, cgroups, per-VM netns, Network Fabric eBPF).

- Dashboard **VM driver** / readiness must show FluxVM reachable (`fluxvm.ok` on `/readyz`).
- VM Dataplane (schema v4) and Edge Maglev Services (Service Fabric v6) require FluxVM dataplane features enabled — see [VM Dataplane](pages/infrastructure/dataplane.md) and [Edge Dataplane](pages/infrastructure/edge-dataplane.md).
- If FluxVM auth is enabled, set `driver.fluxvm_token` in `zyvor-fabricd.toml`.

Sibling checkouts of FluxVM (and GuestKit for image tooling) are expected for full image builds; see the product [README](../../README.md).

## Install sketch

1. Deploy locally or `./scripts/deploy remote USER@HOST`.
2. Confirm `./zyvor-fabricd-ctl status` and `/readyz`.
3. Retrieve admin password → open `https://<host>:9095/sign-in` → `/app`.
4. Follow [Getting Started](getting-started.md).

More detail: [Installation](../getting-started/01-Installation.md) · [Web UI](../getting-started/04-Web-UI.md) · [Production](../deployment/production.md).

## Related

- [Getting Started](getting-started.md)
- [Using the Dashboard](using-the-dashboard.md)
- [Common workflows](workflows.md)
- [Complete page index](PAGE_INDEX.md)
