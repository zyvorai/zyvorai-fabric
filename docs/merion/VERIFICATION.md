---
sidebar_position: 9
---

# Verification

*2026-09-27. What was actually run, where, with what result, and where each claim stops. Nothing here is a promise about a build that has not been
tested. Commands are given so you can reproduce a number; where a check needs a host that cannot be shared, it says so.*

## The lab host

One machine, used for every "live" number below: Intel Xeon E-2336 (12 threads), 31 GiB RAM, Ubuntu 26.04, kernel 7.0, FluxVM with the `node22-agent` cell
template (Node 22, poppler, tesseract), the Merion runtime in Merion mode. Cells are Firecracker/KVM microVMs with a 2 GiB template size. This is a software-test
environment, not a hardware-attested one ([SECURITY-PROFILES.md](SECURITY-PROFILES.md)).

## Automated checks

| Suite | Result | Run with |
|---|---|---|
| Runtime unit tests | **533 passed**, 0 failed (2026-10-16) | `cargo test --manifest-path agent-runtime/Cargo.toml --lib` |
| Runtime lint | clippy `-D warnings` and `cargo fmt --check` clean | CI `keep-unit` |
| Pack lint | every shipped use-case pack and the contributor template validate with the runtime's own checks | `cargo test --lib every_shipped` |
| Use cases end to end (stub cell) | **170 checks passed** | `bash agent-runtime/tests/demos-ci.sh` |
| Solvor client library | **83 tests passed**, 0 failures, 6 skipped (they need a live host) | `swift test` in `integrations/macos-merion` |
| Web console | **419 tests passed** | `npx vitest run` in `web/` |
| SDK | **88 tests passed** | `npm test` in `sdk/agent-runtime` |
| Shell tooling | `merion-up.sh` (8 checks), the template bake (3), shellcheck | `agent-runtime/tests/merion-up.sh`, `bake-fresh-image.sh` |

The stub-cell suites run the fixed extractors as ordinary processes: they prove the runtime, the packs and the API, **not** isolation.

## Live checks on real cells (lab host)

| Check | Result | Run with |
|---|---|---|
| Every shipped use case and scenario, each in its own cell, egress count asserted | **62 passed, 0 failed** (two earlier full passes: 57/57, 62/62) | `./scripts/merion-live-scenarios.sh` |
| Two users, real cells: isolation, revocation, operator routes closed to user tokens, the reference gateway | **14 passed**; the phone-signed approval step is skipped without `MERION_POLICY_SEED` (18 pass with it, earlier run) | `./scripts/merion-live-tenancy.sh` |
| AG-UI run of `echo-agent` on a real cell (agent session with guest networking) | One run by hand: 7 events, valid against `@ag-ui/core` 1.0.0; not in CI | [AGUI.md](AGUI.md) |
| OCR of a photo in a real cell | `receipt-photo` read a generated receipt image | part of the scenarios |
| Concurrency, before the create gate (csv-clean, 20 runs per pass, concurrency 4) | **3, 3 and 3 of 20 failed** in three passes: FluxVM handed two VMs the same nbd device or mismatched a guest-agent token | a one-off loop, see PR #210 |
| Concurrency, after the create gate (same load, 3 passes) | **0 of 60 failed**, outbound count 0 in every run | same loop |
| Official benchmark on the gated runtime | see below | `./scripts/merion-bench.sh --runs 8 --concurrency "1 2 4"` on the host |

### Latency (cold cell per run, csv-clean, 8 runs per level)

| Concurrency | ok / failed | p50 | p95 | Runs per minute | Lowest free memory |
|---|---|---|---|---|---|
| 1 | 8 / 0 | 13.8 s | 16.4 s | 4.3 | 21.5 GiB |
| 2 | 8 / 0 | 17.6 s | 21.3 s | 6.6 | 20.4 GiB |
| 4 | 8 / 0 | 32.0 s | 43.5 s | 6.3 | 19.8 GiB |

Every run boots a fresh cell, so these are **cold** numbers. Creating cells is serialised on purpose (FluxVM cannot provision two VM disks at once), so throughput
plateaus at about 6 runs a minute on this host regardless of concurrency; warm pools, hibernate and resume, and long soaks are **not measured**. Do not size a fleet
from a short run.

## After the Keep → Merion rename (2026-10-16)

The rename changed wire names (`X-Merion-*` headers, `merion-approval-v1`, `/app/merion`, `.merionpack.json`), env vars, the Helm chart,
Keychain services and the iOS bundle id. This is what was run afterwards, on `main` at `c2d6a42a`.

| Check | Result | Run with |
|---|---|---|
| End to end against the FluxVM stub | **85 passed, 0 failed** | `./scripts/merion-e2e.sh` |
| Speculate → approve → apply, and speculate → reject, on the stub | applied only after approve; reject never applies | `./scripts/demo/run-speculate.sh` |
| iOS app | builds for the simulator, unsigned; **not run** | `xcodegen generate && xcodebuild -scheme MerionPhone -destination 'generic/platform=iOS Simulator' CODE_SIGNING_ALLOWED=NO build` in `integrations/ios-merion` |
| Website | `npm run build` succeeds (it failed after the rename until the docs category id was fixed in #377) | `npm ci && npm run build` in `website/` |
| fabricd on Linux | compiled on the lab host during `deploy-merion.sh`; `/health` 200 | `./scripts/deploy-merion.sh user@host` |
| Redeploy of the Merion runtime | services active, `/healthz` 200, `/v1/merion/status` 401 without a token, the CSV smoke test ran in a sealed cell with **0 CONNECT** | `./scripts/deploy-merion.sh user@host` |

### Signed approvals on a host, with required signatures on

`ZYVOR_AGENT_REQUIRE_DEVICE_SIGNATURE=1` was set on the lab host. A throwaway user and a P-256 device (the SDK reference phone, over an SSH tunnel)
tried each path against the live runtime: **14 of 14 checks behaved as expected**.

| Attempt | Result |
|---|---|
| `GET /v1/inbox` announces `format: keep-approval-v1`, `formats: [keep-approval-v1, merion-approval-v1]` | yes |
| Decision with a user token and no signature | **403** |
| Valid signature, but the decision flipped after signing | **403** |
| Signature from a key that is not enrolled for the user | **403** |
| The approval after those three | still pending |
| Valid `merion-approval-v1` approve / deny | **200** / **200** (audit row `format=merion-approval-v1`) |
| Valid legacy `keep-approval-v1` approve | **200** (audit row `format=keep-approval-v1`) |
| Replay of an accepted decision | **409** |
| Operator token, no signature | **200** (the operator is exempt by design) |
| After cleanup: device removed, user tokens revoked, old token reused | **204**, then **401** |

Limits: the signer was a Node script, not a phone or the Solvor app, and not a Secure Enclave key. A real device, a real push relay and the
Secure Enclave key migration after the rename are **not verified**. The enrolled test device is removed; the test sessions and approvals stay in the
append-only journal.

### Auto-approve policies

Stage one (`agent-runtime/src/changeset_policy.rs`) judges a speculate changeset against a policy and journals `changeset.policy.dry_run`. Stage two
(operator-set policies per user and agent, `/v1/users/{id}/changeset-policies/{agent}`, `merionctl changeset-policy`) decides: a changeset inside the
policy takes the same approve-then-apply path as a person's approval. Covered by unit tests and 21 checks in `merion-e2e.sh` against the FluxVM stub
(no policy waits, a user token cannot set one, a bad policy is refused, inside the policy applies, a tightened policy leaves it pending, delete). The
lab-host run is in the section at the end of this page: the routes work and a required phone signature wins, but a real cell reported open egress, so
**approve and apply through a policy has not run on real FluxVM**. See [the design note](design/auto-approve-changesets.md).

### Not run

CI results for `c2d6a42a` (the runs were queued when this was written); grants, confined exec, balloon and branching on a real FluxVM (need a newer
guest image and, for branching, a reflink host); speculate approve and deny against a long-lived session on the lab host (a human deny ran there; a policy approve did not, see the last section).

## Feature matrix

| Feature | Evidence | Boundary |
|---|---|---|
| Sealed use-case cell, deny-all network | Applied by the host before any guest work; the run fails closed and deletes the cell if it cannot be applied; live scenarios pass on real cells | The egress count is a cross-check, not the guarantee ([THREAT-MODEL.md](THREAT-MODEL.md)). Evidence class `software-test`: the operator can read cell memory |
| 60+ use cases | Each has a validated spec; those with a sample run in `demos-ci.sh` and on real cells | The bank packs, the two PDF packs and the OCR packs have not been run on real customer files |
| OCR (photos and screenshots) | Real tesseract through the runtime in `demos-ci.sh`; `receipt-photo` in a real cell | English only, no HEIC, no scanned PDFs; check amounts against the original |
| Tenancy | 14 live checks, unit tests | Not a hostile-tenant proof against a compromised host |
| Approvals signed on a device | Test vectors in both formats (MerionKit and SDK tests); live on the lab host with signatures required: unsigned, flipped and forged decisions refused, valid ones accepted (see above) | Signed by a script, not run in the app or on a phone against a waiting approval ([TODO.md](TODO.md)) |
| Solvor (macOS 26) | Builds, 55 unit tests, connected to a real host, watched-folder flow end to end, email pipeline in real cells | Browser email on real webmail, Siri, microphone, Services, `keep://`: built, **not verified** ([VERIFY.md](https://github.com/zyvorai/solvor/blob/main/docs/VERIFY.md)) |
| Local demo (simulator) | Runs on a Mac end to end; every result labelled `SIMULATED, not sealed` | Not isolated, by design |
| One-command host (`merion-up.sh`) | 9 checks with fake facts. On a clean Ubuntu 24.04 VM (nested KVM, 4 vCPU, 8 GB, 2026-09-26) the real flow was run in stages, fixing what broke: rustup under sudo, missing C toolchain, FluxVM build dependencies (pkg-config, libsystemd-dev, clang, libbpf-dev; no OpenSSL), a static guest agent, starting FluxVM (its unit needs `/run/netns` and `/var/lib/kubelet`), the template bake (inputs staged under `/var/lib/fluxvm`, a failed build no longer leaves a half-built image, phase order). A run of `sudo ./scripts/merion-up.sh` with FluxVM's AppArmor profile enforced and fixed ([zyvorai/fluxvm#108](https://github.com/zyvorai/fluxvm/pull/108) and [#110](https://github.com/zyvorai/fluxvm/pull/110)) exited 0: template baked, runtime deployed, `csv-clean` and `pdf-brief` ran in sealed cells with 0 CONNECT, a token was printed; a second `--dry-run` was all ok. On that same host `./scripts/merion-live-scenarios.sh` then ran **66 passed, 0 failed** in real cells (a first pass before the SDK step was added failed its pack deploys with "install the SDK first"; `merion-up.sh` now runs `npm ci` for the SDK) | Not one uninterrupted pass from an empty machine (FluxVM was already built when the last run started); before FluxVM #108 and #110 its AppArmor profile blocked the image build and cell creation (#110 was open when this was written); a few harmless capability denials (`sys_module`, `audit_write`) remain; Ubuntu 24.04 only; Solvor not connected; evidence class `software-test` |
| Page tracking (`merion-watch.sh`) | Against a real runtime and a local page in `demos-ci.sh` | cron, launchd and notifications not run |
| Template rebuild | `--force` builds a new image beside the old one (3 checks; done by hand on the lab host once, then scripted) | Not run again against real FluxVM since scripting it |

## Known and unresolved

- An intermittent FluxVM eBPF refusal (`bpftool prog load`) was seen in 4 of 56 live scenarios on one day and has not reproduced in five full passes and 40 sequential runs since.
  It fails closed. The concurrency failures above were a **different** FluxVM/guestkit bug (two creates handed the same nbd device), reported as [fluxvm#104](https://github.com/zyvorai/fluxvm/issues/104) and fixed in guestkit 1.2.5; the runtime's create gate now defaults to 4. The table row shows the numbers from before the fix; the benchmark after it has not been re-run here.
- A stale `qemu-nbd` holds the old template image on the lab host, which is why rebuilds go to a new image.
- CI: `CI / test` was red on newer clippy lints until PR #203; `Lab deploy` fails because its SSH login to the workflow's host is refused (the owner's secret to fix).

## Reproduce it

```bash
cargo test --manifest-path agent-runtime/Cargo.toml --lib
bash agent-runtime/tests/demos-ci.sh
# on a FluxVM host with the node22-agent template:
MERION_API=http://127.0.0.1:9096 MERION_TOKEN=... ./scripts/merion-live-scenarios.sh
MERION_API=http://127.0.0.1:9096 MERION_TOKEN=... ./scripts/merion-live-tenancy.sh
MERION_API=http://127.0.0.1:9096 MERION_TOKEN=... ./scripts/merion-bench.sh --runs 8 --concurrency "1 2 4"
```

## Auto-approve policies on the lab host (2026-10-16)

| Check | Result |
|---|---|
| Policy routes (`PUT`/`GET`/`DELETE`, bad policy, no token) | 200 / 200 / 204, 400, 401 |
| Required phone signature with a matching policy | not auto-approved, journaled `a phone signature is required` |
| Signature not required, matching policy, real FluxVM cell | **not auto-approved**: FluxVM reported `egress: unrestricted`, journaled `egress is unrestricted` |
| Approve then apply through a policy on real FluxVM | **not proven**; only the FluxVM stub has run it (`merion-e2e.sh`, 85/85) |

