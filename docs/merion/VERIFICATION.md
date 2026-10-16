---
sidebar_position: 9
---

# Verification

*2026-09-27. What was actually run, where, with what result, and where each claim stops. Nothing here is a promise about a build that has not been
tested. Commands are given so you can reproduce a number; where a check needs a host that cannot be shared, it says so.*

## The lab host

One machine, used for every "live" number below: Intel Xeon E-2336 (12 threads), 31 GiB RAM, Ubuntu 26.04, kernel 7.0, FluxVM with the `node22-agent` cell
template (Node 22, poppler, tesseract), the Keep runtime in Keep mode. Cells are Firecracker/KVM microVMs with a 2 GiB template size. This is a software-test
environment, not a hardware-attested one ([SECURITY-PROFILES.md](SECURITY-PROFILES.md)).

## Automated checks

| Suite | Result | Run with |
|---|---|---|
| Runtime unit tests | **403 passed**, 0 failed | `cargo test --manifest-path agent-runtime/Cargo.toml --lib` |
| Runtime lint | clippy `-D warnings` and `cargo fmt --check` clean | CI `keep-unit` |
| Pack lint | every shipped use-case pack and the contributor template validate with the runtime's own checks | `cargo test --lib every_shipped` |
| Use cases end to end (stub cell) | **170 checks passed** | `bash agent-runtime/tests/demos-ci.sh` |
| Solvor client library | **82 tests passed**, 6 skipped (they need a live host) | `swift test` in `integrations/macos-merion` |
| Web console | **419 tests passed** | `npx vitest run` in `web/` |
| SDK | **77 tests passed** | `npm test` in `sdk/agent-runtime` |
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

## Feature matrix

| Feature | Evidence | Boundary |
|---|---|---|
| Sealed use-case cell, deny-all network | Applied by the host before any guest work; the run fails closed and deletes the cell if it cannot be applied; live scenarios pass on real cells | The egress count is a cross-check, not the guarantee ([THREAT-MODEL.md](THREAT-MODEL.md)). Evidence class `software-test`: the operator can read cell memory |
| 60+ use cases | Each has a validated spec; those with a sample run in `demos-ci.sh` and on real cells | The bank packs, the two PDF packs and the OCR packs have not been run on real customer files |
| OCR (photos and screenshots) | Real tesseract through the runtime in `demos-ci.sh`; `receipt-photo` in a real cell | English only, no HEIC, no scanned PDFs; check amounts against the original |
| Tenancy | 14 live checks, unit tests | Not a hostile-tenant proof against a compromised host |
| Approvals signed on a device | Signing checked against the runtime's test vectors (MerionKit and SDK tests) | Not run in the app against a waiting approval ([TODO.md](TODO.md)) |
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
