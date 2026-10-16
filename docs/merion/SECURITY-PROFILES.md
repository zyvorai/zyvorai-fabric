---
sidebar_position: 3
---

# Security profiles

Keep never labels a cell with more evidence than it has. The profile is set on the
FluxVM side (Phase 6 `security_profile`); Keep reads it and says so in the cockpit.

| Profile | Evidence | Hardware attestation? | What you may claim |
|---|---|---|---|
| `standard` | none | no | Nothing — no evidence class is attached |
| `measured` | `software-test` | **never** | A software-test measurement; the host can still see the VM |
| `confidential-snp` / `confidential-tdx` | `sev-snp` / `tdx` only after a verified hardware run | gated | Unread-by-operator — **not claimable until the launch flags flip** |

## The attestation receipt

The cockpit `attestation` object (console Keep view and `/keep/cockpit`) carries:

| Field | Meaning |
|---|---|
| `profile` | The profile the cell launched with |
| `image_hash` | Soft image hash (not a hardware measurement) |
| `evidence_class` | `none`, `software-test`, or — after a verified run — `sev-snp` / `tdx` |
| `snp_launch_verified` / `tdx_launch_verified` | `false` until one real hardware launch flips FluxVM `security.snp_launch_verified` / `tdx_launch_verified` |
| `host_recover_allowed` | Whether an operator recover path exists |
| `operator_can_read` | Whether the operator can read the guest |
| honesty text | Plain-language statement of the above |

## Rules the product enforces

- **No host recover on confidential.** `POST /v1/sessions/{id}/host-recover` is always
  403 when the profile is confidential or a confidential launch is active.
- **Measured and standard need dual keys** (`ZYVOR_AGENT_RECOVER_KEY_A` + `_B`) for a
  recover grant; the grant is audited and still labelled `software-test`.
- **User-held unwrap stays refuse-closed.** `POST /v1/vault/user-held/complete` returns
  403 until the verified flags are true.
- **Host guest-agent channel helpers refuse** when a confidential launch is active.

## Honesty

Measured is `software-test`, not a TEE claim: *the host can still see a measured VM.*
That is intentional. Until Keep 0.2 runs on real SNP/TDX hardware with a user-held key,
Keep does not claim the operator cannot read the guest. Muse's Secure VM is described
as having the same limit today; public detail on Muse is thin, so treat that as a
characterization, not an audit.

See also [Keep 0.2](MERION-0.2.md), [confidential agent VMs](../design/confidential-agent-vms.md),
and FluxVM's [security profiles](https://github.com/zyvorai/fluxvm/blob/main/docs/security-profiles.md).
