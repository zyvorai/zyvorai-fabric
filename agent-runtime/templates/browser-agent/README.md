# browser-agent template

A FluxVM sandbox template with Node.js, Chromium (loopback CDP), and the Keep
a11y driver (Playwright private). Sized like a per-user desktop-style agent VM
(2 vCPU, ~7.7 GB).

```json
{
  "template": "browser-agent",
  "resources": {"vcpus": 2, "memory_mib": 7900},
  "home_volume": {"name": "browser-home", "guest_path": "/home/agent", "per_user": true},
  "confinement": "strict",
  "egress_mode": "sentinel",
  "egress_allow_hosts": ["example.com"],
  "browser_port": 9222
}
```

## Guest services

| Port | Process |
|---|---|
| `127.0.0.1:9222` | Chromium CDP (`chromium-cdp.service`) |
| `127.0.0.1:9230` | a11y driver (`browser-driver.service` → `driver.mjs`) |

Host reaches both via FluxVM sandbox HTTP/WS bridge. CDP is **not** on a public NIC.

## Build

Same flow as Tutorial 11, on the FluxVM host. Prefer:

```bash
curl -fsSL -o /tmp/node20.tar.xz \
  https://nodejs.org/dist/v20.18.1/node-v20.18.1-linux-x64.tar.xz
(cd /tmp && npm pack playwright-core@1.49.1 && mv playwright-core-*.tgz playwright-pack.tgz)
./scripts/merion-bake-browser-agent.sh
```

Pins: Node `v20.18.1`, `playwright-core@1.49.1`, Debian `chromium` + `chromium-driver`.
The base is Debian 12 because Ubuntu's `chromium-browser` is a snap stub.

CI smoke (no KVM): `./scripts/merion-bake-browser-smoke.sh`.

## Keeping browser traffic on the broker

The guest starts with `ZYVOR_EGRESS_PROXY` set to the runtime's HTTPS CONNECT
proxy. See [`docs/merion/browser/DRIVER.md`](../../../docs/merion/browser/DRIVER.md).

- Only `CONNECT` to `ZYVOR_AGENT_PROXY_CONNECT_PORTS` (default `443`); plain `http://` refused.
- Deploy with `"confinement": "strict"`. Verify:
  `curl --noproxy '*' https://example.com` from inside a session **must fail**.

## Agent tools

Models call the driver via host MCP `browser_*` tools (or guest HTTP `:9230`).
Playwright stays private — snapshot refs only. See DRIVER.md.
