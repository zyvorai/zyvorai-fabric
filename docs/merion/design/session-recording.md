# Session recording

**Status:** built, default off. Tested against fakes (unit tests and an in-process handler test); not run on Linux or at scale.

## What it is

`GET /v1/sessions/{id}/recording` returns a signed bundle that replays what a session did, in time order: the
journal rows, the event log and the approvals. `POST /v1/recordings/verify` checks a bundle's digest and signature
and says which key signed it.

## Why it is safe to hand out

The bundle is built only from host records, then **masked before it is signed**:

| Source | Kept | Left out |
|---|---|---|
| Journal row | action, phase, time, hashes, the OTLP allowlist of scalar fields (ids, host, method, decision) | free-text `error`/`tail`, bodies, previews, the `subject`; the row counts what it dropped |
| Event | kind, sequence, time, data length and SHA-256 | the data |
| Approval | kind, status, created and decided times | the prompt and the planned action |

Replay shows *what* happened and *when*, not the content. A reader who also holds the content can match it to the
SHA-256.

## Signing

Ed25519 over the SHA-256 of the masked bundle, with the key from `ZYVOR_AGENT_RECORDING_KEY` (64 hex characters). With
no key the endpoint answers 404, because an unsigned recording would prove nothing. The host holds the key; the agent
never sees it. A bundle carries its signer's public key, so verification proves the bundle is intact and who signed
it, not that the signer is one you trust.

A journal row's own `hash` is kept as a pointer into the journal. It was computed over the unmasked row, so it cannot
be recomputed from the bundle; compare it against the journal itself.

## Retention

Recordings are built on request, not stored. They can only be as old as the records they read, so the existing
`ZYVOR_AGENT_EVENT_RETENTION_DAYS` bounds the event part. Each export is journaled as `merion.recording.export` with a
step count only.

## Not done

- The console replay view (a page that steps through a bundle). The data is ready for it.
- Prompt and response capture. Replay alone shows what happened, not why; model calls are not in the bundle.
- A browser or screen video. This records host-side actions, not pixels.
- Key rotation and a trusted-signer list; today the caller decides whom to trust.
