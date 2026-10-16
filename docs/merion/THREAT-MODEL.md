---
sidebar_position: 7
---

# Threat model for one-click use cases

What Merion protects when you drop a file on a use case, against whom, what it does **not** protect, and how to check each claim. It is
written for someone deciding whether to trust the design, so it says what is enforced, what is only observed, and what is missing.
It covers use-case runs (a file in, a summary out). Agent sessions, browsing and approvals have their own pages
([KEEP.md](MERION.md), [confine.md](confine.md), [SECURITY-PROFILES.md](SECURITY-PROFILES.md)).

## Assets

- **The file** you drop, and the text extracted from it.
- **The summary** that comes back, and the run history on the host.
- **Your token** (a scoped user token, `kut1.…`) and, on the operator side, the operator token and the signer key.

## Trust boundaries

| Party | Trusted with | Not trusted with |
|---|---|---|
| **The cell** (throw-away microVM) | Nothing. It sees the file for the length of the run | Any network, any secret, the host filesystem |
| **The host operator** | Everything about the run: file, extracted text, cell memory | (This is the limit. See "What this does not protect") |
| **The runtime** (`agent-runtime`) | Choosing the extractor command, applying the network policy, counting egress, storing results | Anything from the uploaded file: names, bytes and content are data |
| **The pack author** | Choosing a fixed extractor and rules | A command line, a path, a network destination: a pack cannot supply any |
| **The client** (Solvor, `merionctl`, your app) | Holding a scoped token | The operator token |

## Threats and what answers them

| # | Threat | Answer | Enforced by | Check it |
|---|---|---|---|---|
| 1 | A malicious file tries to make the cell phone home | The cell gets a **deny-all** network policy from the host before any guest work; if it cannot be applied the run fails closed (502) and the cell is deleted | The host (FluxVM TC/eBPF), not the guest | `docs/merion/confine.md`; `scripts/merion-live-scenarios.sh` on a FluxVM host |
| 2 | A malicious file tries to run code in the cell | The runtime picks a fixed extractor command and a fixed guest file name; the file is never executed and its name is never used in a command. Output is bounded and defanged (`` ` ``, `\|`, `<`, `>`) | `agent-runtime` (`demo_rules.rs`, `demos.rs`) | unit tests in those files; pack-lint test |
| 3 | A pack tries to smuggle in a command or path | A pack is declarative JSON: rules are a closed set, patterns are regexes limited to 200 characters, and the extractor is an enum, not a string that is run | `CustomDemoSpec::validate` | `cargo test --lib every_shipped` |
| 4 | Text in a file or an email "instructs" the summary | The summaries are extractive (keywords, patterns, counts). No model reads the file unless a use case declares one and the operator's vault allows the endpoint; the model call is made by the host after the cell is finished | `agent-runtime` (`model_call.rs`) | a model pack is refused with 403 until the operator allows its endpoint (`demos-ci.sh`) |
| 5 | One user reads another user's runs | Sessions, artifacts, history and audit slices are scoped to the token's user; operator routes are closed to user tokens; a token in a URL is refused | `agent-runtime` tenancy | `scripts/merion-live-tenancy.sh` |
| 6 | A stolen user token | Tokens are scoped (`read`, `run`, `approve`), expire (60 s to 7 days) and can be revoked per user | `agent-runtime` | tenancy tests, `POST /v1/users/{id}/revoke-tokens` |
| 7 | A secret in a file you were about to upload | The Solvor app scans the name and the first 512 KB for private keys, cloud and token strings and `.env`-style files and asks first. It never echoes what it found | Solvor (`MerionKit/SecretScan`) | MerionKit tests |
| 8 | An approval forged or replayed | Approvals are signed by the user's device key over the exact text (`keep-approval-v1`) with a challenge and an expiry; the host verifies the signature | Runtime + the device (Secure Enclave in Solvor) | `docs/merion/mobile/test-vectors.json`, MerionKit signing tests |
| 9 | Output of one run leaks into the next | The cell is deleted when the run ends; a finished run ends its session, and a frozen cell is kept only for inspection | `agent-runtime` | `merion-live-scenarios.sh` |

## What the egress count is (and is not)

A run reports `egress_connects: 0`. That number counts connections **through the egress broker**. It shows the cell did not use the broker.
It is a cross-check, not the guarantee: the guarantee is the deny-all policy in row 1, applied by the host. Say it that way. A `0` on
a host without that policy would prove nothing, which is why the policy is applied before the file is copied in and the run aborts if it cannot be.

## What this does not protect

- **From the host operator.** The evidence class is `software-test`: the cell has no network, but whoever runs the host can read the file, the extracted text and
  the cell's memory. Run the host yourself, or accept that the operator is trusted. Do not describe this as confidential computing.
- **From a compromise of the host or of FluxVM.** The design assumes the host is sound. A kernel or hypervisor escape defeats it.
- **Hardware attestation.** AMD SEV-SNP and Intel TDX profiles exist in the design and are gated on a verified hardware run that has not happened.
  Until then Merion does not claim the operator cannot read the guest ([SECURITY-PROFILES.md](SECURITY-PROFILES.md), [TODO.md](TODO.md)).
- **Metadata.** The host sees who ran which use case, when, and how big the file was. The run history stores results until they expire.
- **Wrong answers.** Extractive rules can miss or mislabel: a pattern that does not fit your bank's layout, OCR that misreads a digit. Check amounts against the
  original; the bank packs are not yet validated on real exports.
- **The endpoint.** The connection from the client to the host is yours to protect (TLS or an SSH tunnel). The token is only sent in the `Authorization` header.

## Known open issue

An intermittent FluxVM eBPF refusal (`bpftool prog load`) while applying the deny-all policy was seen in 4 of 56 live scenarios on one day and has not reproduced since (four full
passes and 40 sequential runs). The runtime **fails closed** on it: the run is refused and the cell deleted, so it costs availability, not confidentiality.

## Review requests

If you review one thing, review these: the deny-all policy path in `agent-runtime/src/confine.rs` and where it is applied in `demos.rs`; the extractor scripts in
`agent-runtime/src/extractors/`; `CustomDemoSpec::validate` in `demo_rules.rs`; and the tenant scoping in `authz.rs`. Report privately as described in [SECURITY.md](../../SECURITY.md).
