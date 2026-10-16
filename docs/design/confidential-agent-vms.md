# Confidential agent VMs (design spec, not implemented)

Status: partly implemented. Keep product framing: see [../merion/MERION.md](../merion/MERION.md).
The manifest's `confidential: auto|required`, FluxVM's
host detection (`GET /v1/host/confidential`), fail-closed `required`, and status
reporting exist; the QEMU launch for SEV-SNP/TDX, block-device volumes, and the
attested key release below do not, and none of it has run on confidential hardware.

**Keep 0.1** uses FluxVM Phase 6 `security_profile: measured` (software-test
evidence, honestly labeled). **Keep 0.2** is this document's user-held key path
after a hardware run — see [../merion/MERION-0.2.md](../merion/MERION-0.2.md).

Modelled on the "Confidential VM with a user-held key"
that Meta announced for Muse; the public details are thin, so the requirements
below are ours rather than a copy of theirs.

## Goal

A user's agent VM whose memory and disk the host operator cannot read, and whose
disk key the operator never holds at rest. The agent runtime and FluxVM stay
untrusted for confidentiality of guest contents; they remain trusted for
availability and for the brokered egress path (see "What stays visible").

## What exists

`CreateVmRequest` already has `secure_boot` and `tpm` fields, which give a
measured-boot foundation. It has nothing for SEV-SNP or TDX: no confidential
machine type, no attestation report, no launch measurement.

## Requirements

1. **Hardware isolation.** QEMU with `-machine ...,confidential-guest-support=`
   and either `sev-snp-guest` (AMD, host kernel with SNP host support) or
   `tdx-guest` (Intel), an SNP/TDX-capable OVMF, and a template flagged
   `confidential`. Non-QEMU backends cannot be confidential.
2. **Attestation before key release.** Guest boots with an encrypted disk it
   cannot open. A key broker verifies the guest's attestation report (launch
   measurement matches the template's expected value, policy bits forbid debug,
   report is fresh and bound to a nonce) and only then releases the disk key.
3. **User-held key.** The disk key is wrapped to a key the user controls (their
   KMS or a passphrase-derived key held client-side). The runtime stores only
   the wrapped blob. The unwrap step happens at the key broker, after
   attestation, so a host that swaps the guest image gets nothing.
4. **Block-device home.** The current home volume is a host directory shared over
   virtiofs, which the host can read. A confidential home volume must instead be
   a LUKS2 block device (raw or qcow2) opened inside the guest.

## Proposed changes

FluxVM (separate repo):
- `CreateVmRequest.confidential: {tech: "sev-snp"|"tdx", policy}`, QEMU args and
  firmware selection, refusal on hosts without the CPU feature.
- An endpoint returning the guest's attestation report and the launch measurement
  of the template, plus block-device sandbox volumes (`kind: "block"`) with an
  optional LUKS header, as an alternative to virtiofs volumes.

Fabric agent runtime:
- Manifest `confidential: {tech, key_id}`. Deploy validation rejects it together
  with `warm_pool_size`, `idle_hibernate_seconds` (a snapshot would copy guest
  memory out), and virtiofs `home_volume`.
- Session create takes a `wrapped_key` (or a `key_id` the key broker resolves).
  The runtime forwards the attestation report to the verifier and passes the
  released key into the guest over a channel that is not the host exec API.
- `audit.jsonl` records `confidential.attested` / `confidential.key_released`
  with the measurement, so a session's trust decision is reviewable.
- Host exec/fs guest-agent calls are disabled for confidential sessions: they
  are a read channel into guest memory by construction.

## What stays visible

- The egress broker runs on the host and sees plaintext HTTP requests and injects
  credentials, so a compromised host operator can read what an agent sends
  through it. Only the CONNECT proxy (TLS end to end) keeps content private.
  Credential injection therefore has to be off, or moved into the guest, for a
  confidential agent.
- Host CPU, memory size, timing, and the fact and size of network traffic.
- Approvals and the journal record hosts and actions, not guest memory.

## Open questions

- Which CPU family do we target first, and does the test host have it? Nothing
  in this spec can be verified without SNP or TDX hardware.
- Where does the key broker live: in the runtime (simplest, but then the runtime
  is in the trust path for the key) or a separate service the user operates?
- Crash and resume: without snapshots a stopped confidential session loses guest
  memory; is disk-only persistence enough for the target use?

## Suggested order

1. FluxVM confidential machine type and attestation endpoint (hardware needed).
2. Block-device volumes with LUKS.
3. Key broker and attestation verification, then the manifest and session fields.
