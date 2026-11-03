# Merion Browser 0.3

Measured appliance — not Muse consumer chrome. Model proposes `open` / `snapshot` /
`act`; Chromium, CDP, cookies, and the proxy stay host objects.

## Shipped in this cut

| Feat | Surface |
|---|---|
| Split-sight pause | `agent_paused_reason`, `POST …/agent-pause\|resume`, `merionctl session pause` |
| Trajectory-as-code | artifact `kind=browse-script`, `GET …/browser/script`, `merionctl browse replay` |
| Origin taint lattice | tab/clipboard OriginSet IFC; paste across disjoint hosts denied |
| SNI-identity label | `browse.network_identity = merion-browser/{tenant}/{session}`; doctor + Hubble link stub |
| Vault-typed fill | pause during fill-secret; audit `source: vault:name` without value |
| Goal-bound tabs | `goal.allow_hosts` + `POST /v1/goals/{id}/browse` |
| Witness + overlay | scaffold on `act` click (pixel_label / snapshot_text) |
| Profile inspect | `GET …/browser/profile` (cookie *hosts* only) |
| Honesty badge | live JSON on cockpit (`evidence=… · browser=a11y-only · proxy=strict`) |
| Dual cookie jars | `cookie_jar: agent\|operator` — operator jar blocks agent tools |

## Not Muse

No avatar, glasses, Shopify graph, or WhatsApp chat. Approvals stay one-screen /
webhook. Training default off.

## Next (0.4 / 0.5)

- PacketWolf *optional* observe of CONNECT 5-tuple (not required for Merion proof)
- Signed site adapters (`adapters/vcenter.yaml`)
- Confidential cells: CDP only via attested vsock

Host eBPF pin (shipped without PacketWolf): [confine.md](../confine.md).
PDF stage demo: [demos/pdf-brief.md](../demos/pdf-brief.md).

See [DRIVER.md](DRIVER.md) for tool contract.
