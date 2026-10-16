---
sidebar_position: 5
---

# Keep and Solvor: open items that need a person or a resource

> The whole list of what is left, including what is buildable and what needs a design decision, is in [REMAINING.md](REMAINING.md). This page keeps the part that is blocked on the owner.

Everything that can be built and tested without these is done or in flight. This page lists what is **blocked on something only the
owner can provide**, what it unblocks, and how to tell when it is done. The plan behind it: make Keep and Solvor easy to try, easy to
trust, and easy to talk about ([ROADMAP.md](ROADMAP.md) has the product side).

## Needs a resource

| Item | Needs | Unblocks | Done when |
|---|---|---|---|
| **Test `merion-up.sh` from nothing, in one pass** | The same on a clean machine, in a single uninterrupted run (I fixed gaps between runs, so no single pass from an empty VM has been done), plus a real Mac connected through Solvor | On a clean Ubuntu 24.04 VM (nested KVM) every phase now works, but only after fixes made along the way ([VERIFICATION.md](VERIFICATION.md)); FluxVM's AppArmor profile blocked the image build until [zyvorai/fluxvm#108](https://github.com/zyvorai/fluxvm/pull/108) and creating a cell until [#110](https://github.com/zyvorai/fluxvm/pull/110) (verified on the VM in enforce mode; #110 was open when written) | `sudo ./scripts/merion-up.sh --install-fluxvm` ends with a token in one pass, Solvor connected, `demos-ci.sh` green |
| **Cloud-init / compose starter** | The VM above, plus a decision on which cloud | A copy-paste "create a VM, done" path; not written yet because an untested file that spends money is worse than none | Boots a clean VM to a working Keep host with one file |
| **Signed and notarized Solvor** | An Apple Developer ID certificate and an App Store Connect API key or app-specific password, added as repository secrets (never given to Claude) | Installing Solvor without Gatekeeper blocking it; the release workflow is written and waits for these | A tagged release produces a `.dmg` that `spctl -a -vv` accepts on a clean Mac |
| **Approvals push relay** | The same Apple account (APNs key) | Approvals that arrive when Solvor is closed; today they are polled every 20 s while the app runs | A phone or Mac receives a push for a waiting approval |
| **Hardware-attested runs (Keep 0.2)** | Time on an AMD SEV-SNP or Intel TDX host | The evidence class above `software-test`; flips `security.snp_launch_verified` / `tdx_launch_verified` | One verified hardware launch is recorded in [pilot-runs](pilot-runs/README.md) |
| **Lab deploy job in CI** | A working SSH key/secret for the lab host (the workflow targets `80.79.5.173`; the lab box used for testing was `212.8.248.187`). From the owner's Mac, `ssh` and `deploy-remote.sh sus@80.79.5.173` worked on 2026-09-30, so the host is fine; the CI secret is a separate thing that has not been confirmed | A green `Lab deploy` badge | `Lab deploy` passes on `main` |
| **Sealed cells on Apple Silicon (Lima)** | An arm64 cell template and bake script, and FluxVM confirmed to run on arm64; a Mac with an M3 or later on macOS 15+ (nested virtualization); about 15 GiB free disk | Trying a *sealed* Keep host on a Mac through Lima. Today the template and bake are x86_64 only and FluxVM's README says nothing about arm64. Lima on an M-series Mac would also give a real arm64 Linux to test `merion-up.sh`'s preflight | A `merion-up.sh` run inside a Lima VM ends with a sealed csv-clean |
| **Gmail and Calendar: the rest of the live check** | The same test account, and for the last rows a second person and a real phone | The first live run **worked** on 2026-09-27 ([connectors](connectors/README.md#verified-against-real-google)): consent, a read of real unread headers, a draft behind an approval that landed in Drafts, a denied send that never reached Google, and the agenda. Still only tested against the fake Google: an **approved send**, **creating a calendar event**, a second person's own connection, the refresh token being renewed (Google expires it after 7 days in Testing mode), an app past Google's verification, a Workspace account, and a real phone instead of the laptop key | Each of those run once against real Google and recorded |
| **Microsoft 365: live check** *(parked: the owner chose not to use Azure)* | An app registration (public client, redirect `http://localhost`) in a Microsoft Entra directory, and a test mailbox. A personal Microsoft account has no directory, so the Entra admin center refuses it (`AADSTS50020`) until one exists: a free Azure account, a work or school tenant, or a Microsoft 365 developer tenant. The client id is not a secret; refresh tokens are never given to Claude | Everything is built and tested against fakes: scopes, public client, rotating refresh tokens kept per person, the Graph previews, three Outlook agents, and an end-to-end run with a fake Graph ([connectors](connectors/README.md#microsoft-365-outlook-mail-and-calendar)). Nothing has talked to real Microsoft, and `$filter`/`$orderby` and the JSON shapes come from Graph's documentation | One live run: consent, a refresh that returns a rotated token and the next refresh using it, a read of mail and events, and a phone-approved draft |
| **Live checks of the controls added on 2026-09-30** | The runtime's API token on the lab host (`80.79.5.173`), given to the session as `MERION_TOKEN` after opening the tunnel, or a permission rule that lets Claude read it (it was blocked from reading the token) | The policy 409 and acknowledge flow, `format=ocsf`, `policy suggest`, the binary-pin routes, body rules and the guard have only run against fakes and `merion-e2e.sh`; the deployed runtime (built from `main`, smoke-tested) has not been exercised | Each run once against the deployed runtime and recorded in [VERIFICATION.md](VERIFICATION.md) |
| **A real Vault, once** | A Vault dev server (`vault server -dev`), plus a Vault Agent or a Kubernetes cluster for the login methods. None exists on the machines used, so the `vault` source is tested against a mock only | Trust in the Vault source: that a real Vault accepts the token, AppRole and Kubernetes requests and that a revoked token gets exactly one re-login | The documented example reads a KV v2 secret with each auth method, and a revoked token is replaced once; recorded |
| **A lab cell for the cell-side controls** | A FluxVM lab cell running a template with `bubblewrap` and `util-linux` | `binaries` (per-program egress) and the seccomp filter have run on real Linux, not through `contain.sh` in a cell. A Chromium browser agent has never run under the filter | A worker under `inner_container: strict` in a real cell keeps working, a denied syscall gets `EPERM`, `binaries` names the right program, and a browser agent still runs; recorded |
| **A GPU host** | A host with a GPU bound to `vfio-pci` (`POST /v1/host/gpus/bind`), and a QEMU template with a guest driver | GPU cells: the picking logic is tested, the rest is not | A session with `"gpus": 1` starts, sees the device in the guest, and frees it on delete; recorded |
| **A design partner** | An introduction to one phone vendor and one bank operations team | Real feedback; the vendor pilot kit and bank packs are written but unvalidated | A pilot runs with their data shapes |
| **Real (anonymised) bank exports** | Sample NEFT/RTGS return files, NACH return reports, reconciliation exports, UPI dispute mail from a partner | The four bank packs' patterns are generic and unchecked against any real export | Packs pass against real samples, patterns fixed where they miss |

## Upstream

- ~~**FluxVM concurrent create / nbd race**~~ — fixed in guestkit 1.2.5 ([guestkit#35](https://github.com/zyvorai/guestkit/pull/35), [fluxvm#104](https://github.com/zyvorai/fluxvm/issues/104)). Keep’s create gate default is now 4 (`ZYVOR_AGENT_SANDBOX_CREATE_CONCURRENCY`); set to 1 only if pinning an older guestkit.

## Needs you to try it (checklists exist)

- **Solvor paths only you can verify**: [browser email on real webmail, Siri and Shortcuts, Talk to Solvor, Services / menu bar / `keep://`, approving with Touch ID](https://github.com/zyvorai/solvor/blob/main/docs/VERIFY.md). Each is built and unit-tested; none is marked verified until it passes on a real Mac.
- **`merion-watch.sh` on a schedule**: the script is tested against a local page; cron, launchd and desktop notifications are not. Try `--notify` on a page you care about.
- **An AG-UI client**: `POST /v1/agui` validates against the official `@ag-ui/core` schemas, but has not been tried with a real chat client. Point one at it and report what it needs (tool-call events, state).
- **The simulator's amber pill in the running app**: `merion-demo-local.sh` then Solvor; the pill is built and unit-tested, not screenshotted.
- **Photos and scans**: OCR is verified in real cells with generated images. Try real phone photos of receipts and bills and report what it misses (blurry, angled and non-English photos are known weak spots).
- **PDF packs on real documents**: `loan-sanction-letter` and `rbi-circular-brief` have no sample and have never read a real sanction letter or circular.

## Decisions for the owner

- **Posting publicly.** Launch drafts live in [`docs/merion/launch/`](launch/); nothing is posted by tooling. Decide when, where and under which account.
- **`zyvor-web` and `docs/merion/marketing.json`.** Shared with the marketing site; changed only on request.
- **Old branches on your personal mirror** (`ssahani/zyvor-fabric`, about 60, mostly Dependabot) and the unmerged local branches `fix-pr-*`, `review/agent-runtime-v3`, `try-devops`.
- **Major-version Dependabot bumps** in other zyvorai repos (`relay-edge#12`, `atlas#32`, and the crate bumps) and `zorvia#28`, which needs a required review.

## Known and unresolved

- **An intermittent FluxVM eBPF refusal** (`bpftool prog load` refused when applying the deny-all network policy) was seen in 4 of 56 live scenarios on one day. It has not reproduced in four full passes (57/57, 62/62, 62/62, 62/62) and 40 sequential runs since, and FluxVM's journal does not record it (the error only reaches the API response). The runtime fails closed either way. If it returns, capture the untruncated error and `dmesg` at that moment.
- **A stale `qemu-nbd`** (up for about 12 hours, no VMs running) holds the old `node22-agent.qcow2` on the lab host, which is why rebuilds go to a new image now. Disconnecting it is a host operation for the owner.
