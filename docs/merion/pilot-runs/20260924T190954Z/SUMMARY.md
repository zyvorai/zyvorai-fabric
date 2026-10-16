# Keep browser screenshot lab proof 20260924T190954Z

- Host FluxVM: `http://127.0.0.1:7788` (sandbox WS bridge deployed)
- Agent: `keep-cdp-stub` on `node22-agent` with `browser_port: 9222`
- Stub implements Chromium `/json/list` + CDP `Page.captureScreenshot`
- `GET /v1/sessions/{id}/browser/view` → tab `Keep CDP stub`
- `GET /v1/sessions/{id}/browser/screenshot` → **HTTP 200** JPEG (`mode=screenshot`)
- Evidence class: **software-test** (not a TEE claim; no input takeover)

Session id (lab): `dbc6eaee-745d-49af-bbc2-9dbf4a24b1a2`
