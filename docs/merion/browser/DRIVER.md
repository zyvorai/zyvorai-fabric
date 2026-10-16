# Keep browser driver

Brokered Chromium in the Keep cell. The model never sees Playwright, raw DOM, or
passwords. Evidence remains **software-test** until SNP/TDX.

## Ports (guest loopback)

| Port | Service |
|---|---|
| `127.0.0.1:9222` | Chromium CDP (operator listing / screenshot / screencast) |
| `127.0.0.1:9230` | a11y driver (`driver.mjs`) |

Host reaches both via FluxVM sandbox HTTP/WS bridge.

## Agent tools (MCP / `POST …/browser/tool`)

```
browser.open { session_id, url }
browser.snapshot { session_id, interactive? }
browser.act { session_id, op, ref?, text?, key?, dy? }
browser.tabs { session_id }
browser.close { session_id, tab? }
```

Guest JSON: `POST http://127.0.0.1:9230/v1/tool` with `{ "tool": "open"|"snapshot"|"act"|"tabs"|"close", … }`.

Rules:

- No `page.evaluate`, `page.content`, or CDP `Runtime`/`DOM` on the tool surface
- Refs (`@eN`) expire on navigation — snapshot again
- Password-role fill returns `needs_host_fill` → host `POST …/browser/fill-secret`
- `file://` denied when `browser.block_file_url` (default)
- Tools refuse unless `confinement: strict` and `browser.enabled` (default true)

## Operator

```
GET  /v1/sessions/{id}/browser/view
GET  /v1/sessions/{id}/browser/screenshot   # rate-limited ≥2s
WS   /v1/sessions/{id}/browser/screencast
POST /v1/sessions/{id}/browser/tool
POST /v1/sessions/{id}/browser/fill-secret
merionctl browser tabs|shot SESSION
```

Debugger URLs are stripped from listings. Screencast is frames only — **no input takeover**.

## CONNECT proxy limits

- Only HTTPS `CONNECT` to `ZYVOR_AGENT_PROXY_CONNECT_PORTS` (default `443`)
- Plain `http://` refused
- Sees `host:port` only — no path review, no credential inject on CONNECT
- Guests without `confinement: strict` can bypass the proxy — doctor/tool gate fails closed

## Threat model

| Threat | Control |
|---|---|
| Prompt-injected page | Taint + ask after untrusted origin |
| Secret exfil in JS | No evaluate; passwords via host fill |
| SSRF / metadata | Private-net gate + DNS pin + strict confinement |
| Operator drive-by CDP | Strip debugger URLs; listing-only |
| Proxy bypass | `confinement: strict`; tools 403 otherwise |
| Trajectory training | Default off; scoped export token |
| Overclaim confidentiality | `software-test` until SNP/TDX |

## Bake

```bash
./scripts/merion-bake-browser-agent.sh          # lab / FluxVM host
./scripts/merion-bake-browser-smoke.sh          # CI syntax check
```

Pins: Node `v20.18.1`, `playwright-core@1.49.1`, Debian `chromium` + `chromium-driver`.

Product feats (pause, IFC, trajectory, badge): [BROWSER-0.3.md](BROWSER-0.3.md).
