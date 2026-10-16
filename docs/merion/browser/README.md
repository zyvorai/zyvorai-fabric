# Brokered browser

- Chromium remote-debugging HTTP listings only (`/json`, `/json/list`, `/json/version`,
  `/json/protocol`) — no raw DOM dump, no mutating DevTools paths.
- **a11y driver:** guest `driver.mjs` on `:9230`; MCP `browser_*` tools; see
  [DRIVER.md](DRIVER.md).
- Operator **live view**: `GET /v1/sessions/{id}/browser/view` and
  `/keep/browser?session=<uuid>` show open tab titles/URLs. Console Keep session
  (`/app/merion/:sessionId`) polls listing + capability strip. WebSocket debugger
  URLs are stripped from listing responses.
- **Screenshot:** `GET /v1/sessions/{id}/browser/screenshot` — JPEG via host CDP
  (rate-limited ≥2s).
- **Screencast:** `WS /v1/sessions/{id}/browser/screencast` and fabricd
  `WS /ws/sessions/{id}/browser/screencast?token=` — frames only.
- **Host password fill:** `POST …/browser/fill-secret` after vault
  `authorize_resolve` (value never returned to the model).
- FluxVM bridge: sandbox HTTP/WS to guest ports.
- Bake: [`scripts/merion-bake-browser-agent.sh`](../../../scripts/merion-bake-browser-agent.sh);
  CI smoke: [`scripts/merion-bake-browser-smoke.sh`](../../../scripts/merion-bake-browser-smoke.sh).
- Honesty remains **software-test**. Input takeover is not implemented.
