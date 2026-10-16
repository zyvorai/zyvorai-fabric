# Keep 0.2 — gated on hardware

Do **not** claim unread-by-operator until a real SNP/TDX integration run flips
FluxVM `security.snp_launch_verified` / `security.tdx_launch_verified`.

## Required (hardware)

1. **User-held unwrap** — vault opens only after phone or YubiKey unwraps a key
   onto a measured/attested guest (`POST /v1/vault/user-held/complete` stays
   refuse-closed until verified flags are true).
2. Flip FluxVM SNP/TDX verified flags after one hardware launch; evidence class
   may then become `sev-snp` / `tdx`.
3. Align with [confidential-agent-vms.md](../design/confidential-agent-vms.md)
   (confidential machine type, block volumes, key broker).

## Software scaffolding (landed; still software-test)

These ship so the product never mislabels a measured VM:

1. **Attestation receipt** — cockpit `attestation` object: profile, soft
   `image_hash`, `evidence_class`, `snp_launch_verified` /
   `tdx_launch_verified` (false until hardware), `host_recover_allowed`,
   `operator_can_read`, honesty text. Console Keep view + `/keep/cockpit` show it.
2. **No host recover on confidential** — `POST /v1/sessions/{id}/host-recover`
   always 403 when the profile is confidential or confidential launch is active.
   On measured/standard, dual keys (`ZYVOR_AGENT_RECOVER_KEY_A` + `_B`) are
   required; grant is audited and still labeled software-test.
3. **Host guest-agent channel helpers** — `FluxVm::process_for_session` /
   `fs_write_for_session` refuse when confidential launch is active.
4. **Browser screenshot + screencast** — `GET …/browser/screenshot` and
   `WS …/browser/screencast` (frames only; no input) via FluxVM sandbox WS bridge;
   fabricd `WS /ws/sessions/{id}/browser/screencast?token=` for the console.
5. **User-held challenge API** — `POST /v1/vault/user-held/challenge` mints a
   nonce; `complete` reads FluxVM `GET /v1/security/capabilities` and is **403**
   until `snp_launch_verified` / `tdx_launch_verified`. When verified, a
   key-broker **stub** grants a vault lease (wrapped disk key still not done).
   fabricd proxies `/api/vault/status` and `/api/vault/user-held/*`.
6. **browser-agent bake script** — [`scripts/merion-bake-browser-agent.sh`](../../scripts/merion-bake-browser-agent.sh).

Soft scaffolding for Keep 0.2 is **complete**. Until hardware: *the host can still see a measured VM.* That is intentional.
