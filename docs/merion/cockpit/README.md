# Live cockpit

Operator surfaces shipped in Keep 0.1 / 0.2 soft scaffold:

| Surface | What you get |
|---|---|
| `GET /v1/sessions/{id}/cockpit` | Taint, approvals, decisions, `attestation`, `egress_connects`, `drop_reasons`, browser links |
| Console `/app/keep` | One-click PDF brief demo (no session yet) |
| Console `/app/keep/:sessionId` | Goal → task → egress proof → honesty → browser |
| `GET /keep/cockpit?session=` | Minimal HTML for phone/laptop (token in query or form) |
| `GET /v1/sessions/{id}/browser/view` | Sanitized open-tab titles/URLs |
| `GET /v1/sessions/{id}/browser/screenshot` | One JPEG via host CDP bridge |
| `WS /v1/sessions/{id}/browser/screencast` | Read-only frames (`?token=` or Bearer) |
| fabricd `WS /ws/sessions/{id}/browser/screencast` | Same, JWT `?token=` for console |
| `GET /keep/browser?session=` | HTML: tab listing + screencast button |

**Egress proof:** `egress_connects` counts Keep audit `egress.connect` / `ebpf.*`.
FluxVM `drop_reasons` appears when the dataplane is attached. PacketWolf is
optional — see [confine.md](../confine.md) and [demos/pdf-brief.md](../demos/pdf-brief.md).

**Visible taint:** untrusted brokered reads paint `tainted_by`; list which egress
rules just went to `ask`. Hidden eBPF alone is not the product.

**Not yet a full Muse-style split desktop:** terminal pane, file browser, and
input takeover are still product goals — see [browser/README.md](../browser/README.md).

**Attestation receipt (0.1 honest / 0.2 soft):** cockpit returns an `attestation`
object (`security_profile`, soft `image_hash`, `evidence_class`,
`snp_launch_verified`, `tdx_launch_verified`, `host_recover_allowed`,
`operator_can_read`, honesty) from FluxVM `GET /v1/security/capabilities`.
If class is `software-test`, UI must not claim the operator cannot read the VM.
Dual-key host recover: `POST /v1/sessions/{id}/host-recover` — forbidden on
confidential; measured needs `ZYVOR_AGENT_RECOVER_KEY_A` + `_B`.
