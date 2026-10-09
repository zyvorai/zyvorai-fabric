# Changelog

## Unreleased

### Added
- **Keep: speculative execution behind an approval.** `POST /v1/sessions/{id}/speculate` runs a command in an isolated
  copy of the cell and opens a `changeset` approval carrying the diff. A person's approve makes FluxVM approve then apply;
  a deny rejects it. The agent cannot approve. `paths` is required (FluxVM needs existing guest directories for a VM).
  `keepctl speculate` / `keepctl decide`, a diff view in the console's Keep session page, and e2e coverage against the
  FluxVM stub. The FluxVM calls were run against a real FluxVM; Keep's own handler was tested against fakes only.
- **Keep: FluxVM credential grants.** `/v1/sessions/{id}/grants` (POST, GET, DELETE) forward to FluxVM's broker, only for
  credentials whose Keep limits a grant can keep: an `Authorization` header, no approval, method, path or per-user limits,
  hosts inside the credential's host. The secret is sent once and never returned or audited.
- **`fluxvm-client`:** `Idempotency-Key` on create and delete, fork, snapshot, restore, snapshot list and delete,
  `?ready=exec` timing, guest exec `policy` with the guest's `enforcement` result (Keep's `process_confined` fails closed
  when the guest did not enforce what was asked), balloon and PSS memory, CD-ROM eject, and `BackendKind::Vz`.
- **`fabricctl live-fork`, `fabricctl memory`, `fabricctl balloon`** and the fabricd routes behind them
  (`POST /vms/{name}/live-fork`, `GET /vms/{name}/memory`, `GET|POST /vms/{name}/balloon`). Balloon is Beta: FluxVM lists
  it as not live-verified. `live-fork` needs the `flux-vm` backend on the KVM engine; the existing
  `POST /vms/{name}/fork` is a separate disk clone.
- **Demos:** `scripts/record-demos.sh` and `scripts/demo/cast.py` render real command runs (and clearly labelled
  illustrative replays) to GIF, MP4 and WebM under `docs/assets/demos/`. `docs/use-cases.md` says which part to use for
  which problem. `docs/FLUXVM-FABRIC-BOUNDARY.md` lists every FluxVM route Fabric uses and whether it was run live.
- **Design note:** `docs/keep/branching-design.md` explains why Keep sessions are not branched with fork yet.

### Fixed
- `scripts/lib/deploy-common.sh` printed `s: unbound variable` after every deploy.
- The website lockfile no longer carries the Trivy findings (`tinypool` is overridden to 2.x).
- `zyvor-fabricd` registered `POST /vms/{name}/fork` twice during one commit window on `main`, which panicked at
  startup; the new route is `/vms/{name}/live-fork`.

## 0.3.0

### Changed
- **`zyvorctl` is now `fabricctl`.** The Fabric CLI binary, its Rust crate (`backend/fabricctl`), the bash completion
  (`/etc/bash_completion.d/fabricctl`), the CI workflow and the docs use the new name, matching the product and `keepctl`.
  There is no `zyvorctl` alias. `make install`, `scripts/install.sh`, `setup.sh`, `scripts/dist-install.sh`,
  `scripts/deploy-remote.sh` and the uninstallers remove a leftover `zyvorctl` and its completion. The Python SDK
  package and its console script are renamed too (`import fabricctl`, command `fabricctl`), which breaks existing
  imports. Environment variables (`ZYVOR_FABRIC_URL`, `ZYVOR_FABRIC_TOKEN`, and the `FABRIC_*` fallbacks) are unchanged.
  If branch protection requires the old `zyvorctl` CI check, repoint it to `fabricctl`.
- **`fabricctl` finds a TLS daemon without `--server`.** With no `--server`, `ZYVOR_FABRIC_URL` or `FABRIC_URL`, it tries
  `https://localhost:9095` when that port answers a plain-HTTP request like a TLS listener does (a default install serves
  self-signed TLS there), and otherwise uses `http://localhost:9095` (the Docker config). The detection sends no
  credentials and does not touch certificate validation. Before, the plain-`http` default failed against a TLS daemon with
  `invalid HTTP version parsed`. An explicit server is used as given.

### Fixed
- **Use-case cells were not confined on the host.** The runtime looked for the guest's default gateway right after
  creating the cell, before the guest had booted, and skipped the network policy without saying so when it found none,
  so the cell ran with FluxVM's default-allow policy (seen on a lab host: `default_allow: true` for a running cell).
  A use-case cell needs no IP networking, so it now gets a deny-all policy applied by the host, needing no gateway,
  and the run fails closed and deletes the cell if the policy cannot be applied. The "0 CONNECT" a run reports counts
  connections through the egress broker; it is not, by itself, evidence that the cell had no network.
- **A use-case run left its cell alive for 30 minutes.** The run's session stayed "Running" and its cell held
  memory until the sandbox's own lifetime ended, so a busy host filled with idle cells (a lab host stopped answering
  after about 25 runs in 35 minutes). A finished run now ends its session and the terminal-session cleanup loop
  deletes the cell at once; a cell frozen for touching the network is kept for inspection.

### Added
- **Seven more everyday use cases**: `payslip-text`, `kindle-highlights`, `android-call-log`, `insurance-claim-mail`, `takeout-my-activity` (each with a synthetic sample, asserted in `demos-ci.sh` from the real engine's output) and the OCR photo packs `fuel-receipt-photo` and
  `school-fee-receipt-photo` (the fuel pack tested on a generated image; school-fee deploys only). The Kindle ranking first put the `==========` separator above the books; it now reads the title line before each clipping. Catalogue and gallery groups updated.
- **`scripts/keep-chat.py`, a small web chat for a Keep agent over AG-UI.** One static page and one proxied route: the token stays server-side, the agent is fixed by `--agent`, `127.0.0.1` only, foreign Host/Origin refused. Streams the reply, starts a new thread when the agent finished the last, shows approval
  requests as notices and cannot approve. 8 proxy tests, a `demos-ci.sh` check, and the page driven in a real browser. `keep-demo-local.sh` now deploys `echo-agent` (so the chat works out of the box) and always does an incremental build, so it never runs a runtime older than the checkout.
- **`echo-agent`**, an agent with no model, credentials or network, to try `POST /v1/agui` end to end; AG-UI now also passes on events an agent emits with `ctx.emit` as `CUSTOM` `keep.event`. Docs record a real-host finding: agent sessions need IP
  networking on the host (`ZYVOR_AGENT_EGRESS_ADVERTISE_HOST` or tap+netns); on the use-case-only lab host the run ended with the runtime's refusal delivered as a valid `RUN_ERROR`, so only AG-UI's error path is verified on a real cell.
- **`scripts/keep-watch.sh`, page-change tracking with no new server capability.** Fetches a public page on the user's own machine (http/https, 2 MiB, 30 s, no credentials in the URL), reads it with a use case in a cell, and
  reports the runtime's own diff against the last run: BASELINE, UNCHANGED, or CHANGED with the lines (exit 3), `--match TEXT` to alert only when text newly appears, `--notify`, and escalation to exit 4 after repeated failures.
  Tested against a real runtime in `demos-ci.sh` (127 checks); cron, launchd and notifications not run. Recipe in `docs/keep/RECIPES.md`.
- **`POST /v1/agui`, an AG-UI endpoint over Keep sessions.** A chat client that speaks the AG-UI protocol can start an agent in a sealed cell and stream its events; later messages on a thread steer the same session. Through
  the same session code as `/v1/sessions`, so scopes, quotas and tenancy apply (a user token needs `run`; threads are per user). **It cannot approve or deny anything**: an approval request is shown as a `CUSTOM` event and is decided on
  the user's device. The stream is validated against the official `@ag-ui/core` 1.0.0 schemas in `demos-ci.sh` (129 checks) with 8 unit tests and authz tests; no tool-call or state events yet, and not tried with a real chat client.
  `model-agent` also accepts `input.message`. See `docs/keep/AGUI.md`.
- **`keepctl init <name>`**, a one-command start for a new use-case pack: scaffolds a valid pack (rules, a synthetic sample, a README) from `docs/keep/pack-template`, refuses to overwrite and refuses bad names. Tested in `demos-ci.sh`: the scaffold
  deploys and passes its own sample with no edits.
- **`scripts/keep-demo-local.sh`, a two-minute local demo that is honest about what it is.** Starts the FluxVM simulator (`agent-runtime/tests/sandbox_stub.py`) and the runtime on loopback with no KVM, Docker or root, mints a 1-day
  user token and prints how to try a use case and how to connect Solvor. It is **not sealed**: the simulator marks its sandboxes `simulated`, and the runtime then reports `badge.evidence: "simulated"`,
  `badge.sealed: false` and an honesty line that starts `SIMULATED, not sealed` (never the software-test wording). Solvor shows an amber "Simulated, not sealed" pill instead of the proof pill, the console shows a warning,
  and `keep-demo.sh` says so. Tested: runtime unit tests for the badge, a demos-ci check that every simulator run is labelled, a KeepKit test (55 tests) and `keep-demo-local.sh` run end to end on a Mac.
- **`scripts/keep-up.sh`, a one-command Keep host.** Run on a Linux machine: preflight (Linux, x86_64, KVM, systemd, memory, disk, tools; refuses honestly), FluxVM check
  or `--install-fluxvm` (experimental, from source), the Keep runtime via the new `deploy-keep.sh local`, the cell template, then a scoped 1-7 day user token and the exact Solvor
  settings. `deploy-keep.sh` gains a `local` target and no longer requires fabricd. Tested with fake facts and a fake curl (6 checks); not yet run on a clean machine.
- **Bank operations packs.** Six declarative packs for a bank's operations teams: `neft-rtgs-returns`,
  `nach-return-report`, `recon-exceptions`, `upi-dispute-mail`, `loan-sanction-letter` and `rbi-circular-brief`, each
  with a README and (except the two PDF ones) a synthetic sample, plus `docs/keep/BANK-OPERATIONS.md`. Declarative, so
  no runtime change; extractive, with no OCR and no compliance claim.
- **OCR for photos and screenshots.** A new `ocr` extractor reads `.png`, `.jpg` and `.tif` with `tesseract` (English) inside the cell, and two packs use it: `receipt-photo`
  and `bill-photo`. The `node22-agent` template gains `tesseract-ocr`; a template baked earlier refuses an image run with a clear message instead of failing. Quality depends
  on the picture (check amounts against the original); scanned PDFs, HEIC and other languages are not read. Verified with real tesseract through the runtime (a generated
  receipt image and a blank one), not yet in a real FluxVM cell.
- **Solvor**, the Mac app, (`integrations/macos-keep`): a native SwiftUI client. `KeepKit` (API client, Keychain token store, Secure Enclave approval
  signing checked against the runtime's test vectors, secret scan, folder rules, pack catalogue; 33 unit tests, 4 live tests passed against a real
  host) and the app (use cases, runs and compare, approvals, watch folders, menu bar, Services entry, Shortcuts intent, `keep://`). Ad-hoc signed,
  local build only. Verified: connect and list, the client against real cells, and a watched folder running a file end to end; the Shortcuts/Siri
  intent, Services entry, menu bar and URL scheme are built but not exercised. Redesigned with the Zyvor mark, a generated app icon, Liquid Glass on
  macOS 26 and system-accent buttons. **Read an email from the browser** (on click, with a redaction preview; the `.eml` pipeline and routed packs ran in
  real cells; the Apple Events reader is not verified against a real webmail page) and **Talk to Solvor** plus Siri App Shortcuts (voice can never approve,
  deny, send or delete; microphone, Speech, Translation and Siri not run here). 54 unit tests. `docs/keep/MACOS-APP.md`.
- **Eighteen developer-tool, browser and desktop-app use cases**, plus **recipes** for calling Keep from Siri Shortcuts, macOS,
  Windows and other agents. Packs: `github-prs`, `github-issues`, `github-actions-log`, `dependabot-alerts`, `git-log-digest`,
  `xcodebuild-log`, `xcode-crash-log`, `vscode-extensions`, `vscode-settings-audit`, `bookmarks-digest`, `browser-history-takeout`,
  `mac-apps-inventory`, `mac-launch-items`, `windows-services`, `windows-scheduled-tasks`, and three Excel sheets
  (`sales-register-sheet`, `inventory-sheet`, `attendance-sheet`); `chat-export-digest` now also reads WhatsApp's iPhone
  export layout. Declarative and extractive, no runtime change. GitHub, `git log`, `launchctl` and `system_profiler` samples follow
  real output; the others follow documented layouts and are not verified against a real export. `docs/keep/RECIPES.md` documents
  the two calls (run a use case, read the artifact) with a Shortcut, a macOS script, a PowerShell script and webhook triggers.
- **Four office use cases.** `receivables-ageing` and `reimbursement-claims` (from a mail export), `po-line-items` (a PO as
  text) and `employee-ledger` (a CSV), for the paperwork around invoices, POs, staff and claims. Extractive; they list and
  count, and are not books of account, tax validation or payroll.
- **Eight Mac and Windows use cases.** Packs for files exported from those machines: `mac-system-report`,
  `homebrew-audit`, `mac-log-triage`, `mac-update-history`, `windows-systeminfo`, `windows-hotfixes`,
  `windows-installed-software` and `windows-event-log`, each with a README and a sample. Declarative, extractive, no runtime
  change; run live in real cells with `0` outbound connections. The macOS samples follow real command output; the Windows
  samples follow the commands' documented layouts and are not verified against a real Windows export. Serial numbers, host
  names and IP addresses are not echoed. Keep does not connect to or drive the machine.
- **Eight phone-user use cases.** Packs for what a person exports from a phone: `chat-export-digest`,
  `bank-sms-ledger`, `card-statement`, `calendar-week`, `contacts-audit`, `travel-itinerary`, `subscription-finder`
  and `receipt-pdf`, each with a README and (except the PDF one) a sample. Declarative, so no runtime change; they are
  extractive, and one-time codes are not echoed. Run live in real cells with `0` outbound connections. They are listed
  in `docs/keep/SCENARIOS.md` and on the phone-makers page of the GitHub Pages site. Photos and screenshots are not
  covered (no OCR).
- **Keep for phone makers, pictured.** A `/keep/phones` page on the GitHub Pages site (architecture, a user's day
  with a phone mock-up, the phone-signed approval handshake, per-user isolation, the measured benchmark, and a
  built / reference / not-built status list) and five diagrams in `docs/assets/keep/` that the docs embed too.
  `VENDORS.md`, `TENANCY.md` and the mobile guide now show them, and `VENDORS.md` states that phone-signed
  approvals have not yet been run with a waiting agent on a real cell. The Zyvor website is not changed.
- **`scripts/keep-live-tenancy.sh`**: two users, a phone-signed approval and the reference gateway against a
  live shard in real cells (29 checks pass on the lab host).
- **A blueprint and a reference gateway for phone vendors.** `docs/keep/VENDORS.md` (architecture, who runs
  what, what to claim and not claim, questions for counsel), `reference/vendor-gateway` (vendor login in,
  placement by region, scoped user tokens, device enrolment behind a strong login, push relay with adapters,
  per-user rate limit, usage rollup; 12 tests, no dependencies), `scripts/keep-bench.sh` (cold-start and
  concurrency, with a memory guard; first numbers from a lab host are in the doc), and Simplified Chinese
  strings for the navigation, headings and buttons of the Keep pages.
- **The model is the vendor's choice.** A manifest's `model_socket` now works: agents call
  `ctx.model.chat()` (an OpenAI-compatible endpoint, through the egress broker with the vault credential),
  and OpenAI-compatible CLI agents are pointed at it. Deploy checks the URL and that the credential is
  granted. Recipes for Qwen, DeepSeek, GLM and local servers, and a `model-agent` example pack.
  Docs: [MODELS.md](docs/keep/MODELS.md).
- **Phone-signed approvals.** The operator enrols a phone's public key per user (ECDSA P-256 as Android
  Keystore holds it, or Ed25519); the phone signs an exact text naming the approval, the decision, a digest of
  the planned action and a server challenge, and the runtime refuses any decision whose signature does not
  verify (flipped, replayed, forged or late). Mandatory per credential or globally for user tokens. Approvals
  push to each device through a vendor-run relay. Node reference client `keep-phone`, cross-language test
  vectors. Docs: [docs/keep/mobile](docs/keep/mobile/README.md).
- **Many users on one Keep shard.** Operator-minted, per-user, stateless **user tokens**
  (`POST /v1/user-tokens`, scopes `read` / `run` / `approve`, revocable per user) reach only that
  user's sessions, approvals, artifacts and audit rows; everything else is 403 by default and
  another user's objects are 404. Per-user quotas (429) and `GET /v1/usage`, `GET /v1/inbox`.
  fabricd now filters approvals and audit by session ownership for non-admins and adds
  `POST /api/agent-tokens`. Docs: [TENANCY.md](docs/keep/TENANCY.md).
- **`scripts/keep-live-scenarios.sh`** runs the built-ins, the scenario packs, batch, zip, a webhook trigger and
  history against a live runtime in real cells (18 checks).
- **The cell template and seven scenario packs, in the repo.** `agent-runtime/templates/node22-agent/`
  (Ubuntu 24.04, Node 22, poppler, the guest agent) with a one-command bake,
  `./scripts/keep-bake-node22-agent.sh` (`--dry-run` shows what is missing). New use-case packs under
  `examples/keep-agents/`: `status-page-watch`, `mailbox-triage`, `api-facts`, `expense-sheet`,
  `nda-review`, and two model-assisted ones, `invoice-model-brief` and `meeting-notes-model`. Docs:
  [SCENARIOS.md](docs/keep/SCENARIOS.md).
- **Model-assisted use cases.** A use case can declare one `model` step: after the cell
  extracts the text, the host sends it to a single OpenAI-compatible endpoint and adds the
  reply to the artifact. The cell stays offline (0 CONNECT); the vault decides whether the
  endpoint is reachable, the first use needs an out-of-band approval, every call is audited
  (sizes and a digest, never the text or the key), and the reply is sanitised. New
  `GET/DELETE /v1/model-grants`, `keepctl grants`. Docs: [MODEL.md](docs/keep/MODEL.md).
- **More Keep file types and rules.** Use cases can read `.docx`, `.xlsx` (first sheet),
  `.html`, and `.eml` / `.mbox` (fixed Node scripts run in the cell), and a zip upload fans
  out to one cell per accepted file. New rules: `regex_extract` (linear-time `regex`),
  `json_path`, `table`. A scanned PDF now says plainly that Keep does no OCR. Docs:
  [PACKS.md](docs/keep/PACKS.md).
- **Keep triggers and batch.** `POST /v1/demos/{id}` takes several files (one cell each,
  one `batch_id`, 207 when some fail). Signed webhook triggers
  (`POST /v1/triggers/{id}/hook`) and watched-folder triggers (under
  `ZYVOR_AGENT_WATCH_ROOT`) start a use case without an upload. `keepctl run` takes
  several files and `keepctl trigger list|add-webhook|add-folder|rm|fire`; the console
  uploads several files and has a Triggers tab. Docs: [TRIGGERS.md](docs/keep/TRIGGERS.md).
- **Keep run history.** Artifacts can carry `ttl_seconds` (hidden at once, swept on
  the next write); `GET /v1/artifacts` filters by `use_case` and `since`;
  `GET /v1/artifacts/{a}/diff/{b}` returns a line diff of two runs. The approval
  webhook also sends `run.finished` / `run.failed`. New `keepctl`
  `list | run | artifacts | diff | audit | approvals`, fabricd `GET /api/artifacts`
  and `/api/artifacts/{a}/diff/{b}` (admin), and a console page at
  `/app/keep/history` (Runs with diff, Audit with the hash-chain check, Approvals).
- **Keep use cases you build yourself.** A declarative `pack.json` (an extractor
  plus bounded summary rules, no code) is deployed from the console
  (`/app/keep` → **Deploy your own use case**) or with `keepctl deploy`;
  TypeScript agent packs deploy in one command, signed with Node's Ed25519
  (byte-compatible with `keep_sign_policy`). New `keepctl bundle` / `doctor`,
  console **Deploy an agent pack**, `POST /v1/demos` + `DELETE /v1/demos/{id}`,
  `GET /v1/keep/status`, fabricd `POST /api/packs` (admin, signed bytes forwarded
  untouched). Docs: [PACKS.md](docs/keep/PACKS.md),
  [Tutorial 19](docs/tutorials/19-build-your-own-use-case.md).
- **Six more one-click use cases**, table-driven: contract clauses, security
  questionnaire, meeting actions, log triage, SBOM summary, CSV cleanup (with
  spreadsheet-formula neutralisation). `GET /v1/demos`, `POST /v1/demos/{id}`,
  `./scripts/keep-demo.sh`. Docs: [demos/README.md](docs/keep/demos/README.md),
  [Tutorial 18](docs/tutorials/18-keep-use-cases.md).
- **`./scripts/deploy keep user@host`.** Installs Fabric and the Keep runtime on a
  host that already runs FluxVM, registers only the author's public signer key,
  and smoke-tests the CSV cleanup (the PDF demos also need pdftotext in the template).
- **Demos e2e in CI** (`agent-runtime/tests/demos-ci.sh`) against the real runtime
  and the FluxVM stand-in.

### Fixed
- `deny_udp` is carried through the driver-core `VmNetworkPolicy` (the fluxvm
  driver stopped compiling once the client gained the field) and is part of the
  policy fingerprint.
- The Agent Runtime `sessions` job passes again: `session-ci.sh` no longer collides
  with the default CONNECT proxy port and the FluxVM stand-in answers the guest-agent
  ping the runtime now sends. Keep workflow clippy passes (`browser::driver_call`
  visibility).

### Added (earlier)
- **Keep Browser 0.3.** Split-sight `agent_paused_reason`, origin taint lattice,
  trajectory-as-code (`browse-script` artifact), goal-bound tabs, honesty badge,
  profile inspect, SNI-identity label. Docs: [BROWSER-0.3.md](docs/keep/browser/BROWSER-0.3.md).
- **Keep host eBPF via FluxVM (no PacketWolf).** Confinement posts
  `deny_udp` + gateway-only ports + deny metadata/DNS CIDRs; cockpit shows
  `egress_connects` + FluxVM `drop_reasons`; `ebpf_deny` freezes the cell.
  Docs: [confine.md](docs/keep/confine.md).
- **One-click PDF → brief demo.** Pack `examples/keep-agents/pdf-brief/`,
  `POST /v1/demos/pdf-brief`, fabricd `/api/demos/pdf-brief`, console
  `/app/keep` **Brief this PDF**, `./scripts/keep-demo-pdf.sh`. Expect 0 CONNECT.
  Tutorial: [17-keep-pdf-brief.md](docs/tutorials/17-keep-pdf-brief.md).
- **Brokered Keep browser (Chromium + Playwright a11y).** Guest `browser-agent`
  template with loopback CDP `:9222` + driver `:9230`; MCP `browser_*` tools;
  `POST …/browser/fill-secret`; `keepctl browser tabs|shot`; policy `browser:`
  block; bake + CI smoke scripts. Docs: [DRIVER.md](docs/keep/browser/DRIVER.md).
- **Keep 0.2 soft scaffold follow-through.** Cockpit/vault read FluxVM
  `GET /v1/security/capabilities` for `snp_launch_verified` /
  `tdx_launch_verified`; user-held complete fail-closes until verified, then
  grants a vault lease via key-broker stub (wrapped disk key still not
  implemented). Browser screencast WS + screenshot links on cockpit;
  fabricd proxies screencast (`/ws/sessions/{id}/browser/screencast`) and
  vault user-held routes; `scripts/keep-bake-browser-agent.sh`.
- **Keep guest vsock fix (lab).** Musl-static `fluxvm-guest-agent` in `node22-agent`
  image (glibc host binary failed with `GLIBC_2.39 not found`); QEMU and
  Firecracker (`node22-fc` flat rootfs) guest vsock healthy.
- **Vault software unwrap ceremony.** Optional `ZYVOR_AGENT_VAULT_UNWRAP_REQUIRED=1`
  with `POST /v1/vault/unwrap-tokens`, `POST /v1/vault/unwrap`, `GET /v1/vault/status`
  (`secret_backend: host-env` honesty). `keepctl unwrap-token|unwrap|vault-status`.
- **Keep console browser listing.** `/app/keep/:sessionId` polls tab listing;
  fabricd proxies `GET /api/sessions/{id}/browser/view`.
- **Keep 0.1 pilot gate.** Live FluxVM e2e requires a registered agent template
  (no soft-pass); `./scripts/keep-pilot-gate.sh` runs happy + deny paths and
  archives logs under `docs/keep/pilot-runs/`; console Keep view at
  `/app/keep/:sessionId` (goal → evidence → approval → outcome); fabricd proxies
  `GET /api/sessions/{id}/cockpit`. Non-broker approvals can be decided after a
  session ends. Docs: [docs/keep/PRODUCTION.md](docs/keep/PRODUCTION.md),
  [docs/keep/STATUS.md](docs/keep/STATUS.md).
- **Signed agent deployments in Keep mode.** `POST /v1/agents` requires
  `X-Keep-Manifest-Signature` over the exact JSON body when
  `ZYVOR_AGENT_KEEP_MODE=1`; `keepctl create --signature` / `policy sign agent.json`.
- **Keep packaged agents (infra, migration, deploy).** Goals/plans/artifacts API
  (`/v1/goals`, `/v1/artifacts`, advance → `/v1/approvals`); cockpit `active_goal` /
  `recent_artifacts`; shared `_fabric` client + `fabric-api` credential/policy
  recipes; packs `infra-ops`, `migration-op`, `deploy-op` under
  `examples/keep-agents/`; demo `./scripts/keep-pack-demo.sh`. Docs:
  [docs/keep/goals](docs/keep/goals/README.md), Tutorial 16 appendix, STATUS.
- **Keep 0.1 live proof.** Fail-closed Keep mode (`ZYVOR_AGENT_KEEP_MODE=1`) requires
  trusted policy signers at startup and refuses unsigned `keep.policy.yaml` updates;
  `CredentialVault::authorize_resolve` allowlists host/method/path/user before
  host-env secret injection; operator browser live view
  (`GET /v1/sessions/{id}/browser/view`, `/keep/browser`); cockpit reports
  `evidence_class: software-test` and measured `security_profile`; lab gate
  `./scripts/keep-live-lab.sh` / `KEEP_E2E_FLUXVM=1 ./scripts/keep-e2e.sh`.
  Product page at console `/keep`. Docs: [docs/keep/PRODUCTION.md](docs/keep/PRODUCTION.md),
  [Tutorial 16](docs/tutorials/16-keep-workstation.md).
- **Inner containment, always-on workstations and a browser tab view.**
  `inner_container: strict` runs the worker unprivileged in a bubblewrap container
  (fails closed); `persistent: true` plus `PUT /v1/workstations/{agent}/{user_id}`
  keeps a user's session running with backoff restarts; `browser_port` plus
  `/browser/view` give operators a read-only live tab listing; screenshot +
  read-only screencast (`WS …/browser/screencast`) via FluxVM CDP bridge (input
  takeover not implemented).
- **Confidential VMs when the host has them.** `confidential: auto|required` asks
  FluxVM (`feat/sandbox-resources`, `GET /v1/host/confidential`) for a
  hardware-encrypted sandbox and falls back to a normal VM (`auto`) or refuses
  (`required`), recording the outcome on the session. Launch on SEV-SNP/TDX hardware
  is not implemented yet, so today `auto` always falls back.
- **Agent containment.** `confinement: strict` drops all sandbox traffic except to
  the egress broker and proxy (FluxVM eBPF policy, fails closed); approvals are
  pushed to `ZYVOR_AGENT_APPROVAL_WEBHOOK` (HMAC-signed) so a person sees them,
  and a credential can require a per-request `send`/`purchase` approval; per-host
  `egress_rules`, `dlp` secret scanning, and per-session `taint` (tainted sessions
  need approval to write and cannot be auto-allowed by Sentinel;
  `POST /v1/sessions/{id}/untaint`). `fabric-agent deploy` has flags for each.
- **`fabric-agent deploy` flags for the newer manifest fields** (egress mode, home
  volume, per-user home, sandbox size, skills) and `user_id` in the SDK types.
- **Per-user agent VMs.** `home_volume.per_user` gives each `user_id` its own volume
  and its own one-at-a-time session slot, so one deployed agent serves many users;
  `resources` sets vCPUs and memory per sandbox (FluxVM `feat/sandbox-resources`,
  operator ceilings `ZYVOR_AGENT_MAX_VCPUS`/`_MAX_MEMORY_MIB`). Volumes still have no
  size quota.
- **HTTPS CONNECT proxy for browsers.** Sandbox browsers reach the internet through
  the same allowlist, `ask`/`sentinel` review, private-network gate and journal as
  brokered requests (`ZYVOR_AGENT_PROXY_LISTEN`, default `:18083`), and
  `agent-runtime/templates/browser-agent` is a Debian + Node + Chromium template
  recipe. Only effective for guests with no direct route out.
- **Sentinel egress review.** `egress_mode: "sentinel"` has an operator-configured
  reviewer model (`ZYVOR_AGENT_SENTINEL_URL`, `_MODEL`) screen requests to unlisted
  hosts before an operator is asked. It can deny, or escalate to the existing
  approval flow (which is also what happens on any error or when unconfigured); it
  can release a single request only with `ZYVOR_AGENT_SENTINEL_CAN_ALLOW=1`, and
  never grants a session-wide approval. Verdicts are journaled.
- **Agent skills with scoped mounts.** Immutable, content-addressed skill bundles
  (`/v1/skills`, `zyvorctl skill`) that agents pin at deploy time via manifest
  `skills` and mount per session: base skills at `/opt/zyvor/skills`, scoped skills
  at `/opt/zyvor/skills-scoped` only when the operator's
  `ZYVOR_AGENT_SKILL_SCOPES_FILE` policy allows the agent's `skill_scope`. The
  daemon proxies `/api/skills`.
- **Persistent agent home volume.** A manifest `home_volume` mounts a named FluxVM
  volume (default `/home/agent`) that survives sandbox replacement and new agent
  versions. It needs a QEMU-backed FluxVM template, `max_concurrent_sessions: 1`, and
  no warm pool or idle hibernation; a second session while the volume is attached
  gets `409`. Requires FluxVM sandbox volumes (`zyvorai/fluxvm` `feat/sandbox-volumes`).
- **Agent approvals and a tamper-evident action journal** in the agent runtime.
  Approvals now carry a `kind`, `subject`, and `planned_action`; approvals and
  brokered egress calls are appended to a SHA-256 hash-chained `audit.jsonl`
  (`GET /v1/audit`). Agents can opt into `egress_mode: "ask"`: a request to a host
  outside the allowlist is held while an operator approves it once or for the
  session (`zyvorctl approval approve --scope session`), and is refused on denial,
  timeout, or session end. The daemon proxies `/api/approvals` and
  `/api/audit/agent-actions`; `zyvorctl approval` and `zyvorctl agent-audit` read
  them.
- **AI Workloads are Beta on a single cluster** (multi-site HA store stays
  Preview; this is not GA). The control plane now
  records revisioned rolling, canary, and blue/green rollouts; content-addressed
  model jobs with format scanning and derived artifacts; hierarchical gateway
  request and token windows, plus in-flight stream caps; optional HTTPS or mTLS to
  replicas; batch jobs that stay queued until a ready replica is claimed; GPU
  temperature and ECC filters on reported fields; an HMAC secret file; inference JWTs
  whose audience is `fabric-inference`; a Janus inventory path that does not call
  the NVIDIA driver (`FLUXVM_AI_JANUS_URL`, optional `FLUXVM_AI_JANUS_API_KEY`);
  an admission webhook that stays off until enabled; key rotation with an overlap; deploy-hour policy;
  a token-budget alert; and a SHA-256 audit chain. `highest_throughput` and
  `lowest_failure` are Maglev strategies. Terraform can manage an AI site.
  Known runtimes (vLLM, TensorRT-LLM, Triton, llama.cpp, TEI) launch by default;
  `FLUXVM_AI_DENY_RUNTIMES` blocks a name and `FLUXVM_AI_ALLOW_RUNTIMES` is an
  optional strict allowlist. One process is one Raft voter when `FLUXVM_AI_RAFT_ID`,
  `FLUXVM_AI_RAFT_PEERS`, and `FLUXVM_AI_RAFT_TOKEN` are set; unset peers leave
  placement unchanged, and `FLUXVM_AI_CONSENSUS=external` is only a label. A Linux
  heartbeat reads `nvidia-smi` temperature, power, and ECC when the binary is on
  `PATH`, and leaves those fields unchanged when it is not. PCI MIG create runs
  `nvidia-smi mig` when `FLUXVM_AI_PCI_MIG=1` (inventory-only with
  `FLUXVM_AI_PCI_MIG_RECORD_ONLY=1`); without the flag a PCI parent still returns
  400. Janus MIG records stay driver-free. `AiSite.peer_url` streams a verified
  model digest and stays pending until the peer returns 201. Inference
  `Upgrade: websocket` is bridged to the selected upstream. `FLUXVM_AI_OTEL_ENDPOINT`
  exports one gateway span. The operator watches an AI CRD only when that CRD is
  installed. Rate counters and the audit tip stay in the local file until Raft
  peers are set; then the leader applies them and a follower forwards instead of
  keeping a second count.
  GitHub Actions `ai-workloads.yml` covers unit tests, dry-run smoke, the
  admit webhook Helm templates, and Agent Runtime credential tests.
  Maglev accepts an IP hostport from a Janus replica and skips the upsert
  when every ready replica is a Janus or `dry-run-*` device so the gateway
  can proxy alone. Lab k3s can enable the InferenceDeployment admit webhook
  against host fabricd with the chart `admissionWebhook` values. The console
  Nodes tab and `zyvorctl ai node list` list Janus inventory; MIG slice create
  and delete are `zyvorctl ai node mig-create|mig-delete`.
  `scripts/smoke-ai-janus-lab.sh` covers health, chat, MIG, and admit on a
  Janus-enabled lab. Remote deploy also refreshes `/usr/local/bin/zyvorctl` so
  lab PATH does not keep a stale binary. Operator docs include Tutorial 15
  (`docs/tutorials/15-ai-workloads.md`) and the website manual page
  `/docs/zyvor-fabric-manual/ai-workloads`.
  See [docs/ai-workloads.md](docs/ai-workloads.md).
- **zyvorctl Cilium-style CLI UX** — grouped colorful `--help` (Basic / Dataplane /
  Networking / Meta, with emoji section markers), `--color auto|always|never`,
  `--server` / `--token`, meta commands `status` / `config` / `completion`
  (bash/zsh/fish), and table-mode output that no longer dumps JSON for object
  responses. Hubble `--style` defaults to color on TTY.
- **Deploy Agent from the web console** — the Agents page can now deploy a
  new agent directly (upload a bundle built with the new `fabric-agent
  build` CLI subcommand, set template/credentials/egress hosts/warm pool
  etc.), instead of requiring the agent-runtime CLI. New credential-free
  `ops-agent.ts` example alongside the existing LLM-calling one, and
  [Tutorial 13](docs/tutorials/13-deploy-agent-from-console.md) walking
  through the whole flow. `sdk/agent-runtime`'s CI now smoke-builds both
  examples through the real `fabric-agent build` command.
- Weekly + on-demand CI smoke test for NousResearch/hermes-agent's
  installer, as a candidate real-world agent workload for Fabric-hosted
  VMs/sandboxes ([.github/workflows/hermes-agent.yml](.github/workflows/hermes-agent.yml)).
- **Agent sessions in CI** — `.github/workflows/agent-runtime.yml` starts
  the runtime against a FluxVM stand-in and runs a real session: ops
  health check, MCP, cron, signed webhooks, a one-run loop, delegation,
  a fake Claude approval, and a Go agent that returns its source and
  prints `hello`. No model provider is called.
- **Lab deploy workflow** — a push to `main` runs
  [`.github/workflows/lab-deploy.yml`](.github/workflows/lab-deploy.yml),
  which rebuilds `zyvor-fabricd` on the lab host and runs
  `--e2e --verify-apis`. Login uses the `LAB_DEPLOY_KEY` secret.
- **Coding-agent harness, schedules, and MCP** — an agent version can
  run the `claude`, `codex`, or `gemini` CLI inside the sandbox with
  keys still injected by the host broker. Operators approve a
  `ZYVOR_APPROVAL` line, sessions can delegate to another agent, and
  the runtime accepts cron, HMAC webhooks, bounded loops, and MCP
  `list_agents` / `list_executions` / `chat_with_agent`.

### Fixed
- **The Alerts page's CPU/memory/disk rules never actually fired** — `GET
  /api/system/alerts` only ever returned live bandwidth alerts from
  `net_monitor`; the `cpu-high`/`mem-high`/`disk-high` rules shown on the
  same page under "Alert Rules" were seeded into the store once and then
  just echoed back verbatim by `/api/system/alerts/rules` — nothing ever
  read a real CPU/memory/disk value and compared it against them. A host
  pinned at 100% CPU for a week would never produce a "High CPU usage"
  alert. `get_system_alerts` now also evaluates each enabled rule against
  the same `/proc`-derived metrics `/api/system/metrics` already exposes,
  and both alert sources are normalized into the one shape the frontend
  actually reads (`id`/`severity`/`title`/`message`/`value`/`timestamp`) —
  bandwidth alerts previously serialized with none of those field names
  (`triggered_at` instead of `timestamp`, no `message`/`title` at all), so
  a bandwidth alert firing would have rendered as a badge with no text.
  5 new tests. Verified live: real cpu/memory/disk metrics on a running
  host correctly produce no alerts while under threshold, and the same
  evaluation path (exercised with a trivially low threshold) correctly
  fires against the real live metric.
- **agent-runtime**: a "Run session" could get stuck showing `creating`
  forever, with no error, no matter how it was retried. `create_session`
  awaited the whole guest-provisioning flow (boot wait, health-check
  retries, bundle push — which can legitimately take minutes) inline,
  before responding to the HTTP request that started it. Any upstream
  timeout shorter than that — the browser, or `zyvor-fabricd`'s own reverse
  proxy — closed the connection first, and axum then dropped the
  in-flight handler future outright, skipping every line of the
  failure-handling code that would have marked the session `Failed` and
  released its sandbox. The session was left in `creating` with its
  `updated_at` identical to its `created_at`: nothing ever touched it
  again. Provisioning now runs in a detached `tokio::spawn` task; the
  initial response returns as soon as the sandbox is admitted, and the
  eventual `session.running`/`session.failed` outcome is always recorded
  regardless of what the original caller does. Every individual FluxVM
  call inside that provisioning flow (`process`/`fs_write`/`guest_request`)
  is now also independently bounded by a 15s timeout — the existing 120s
  admission-time timeout only ever gets checked *between* retry-loop
  iterations, so a single hung call could previously block the loop from
  ever reaching its own deadline, no matter how short that deadline was.
  The web console's session detail view now polls every 2s while a
  session is non-terminal, since the backend fix means that transition is
  genuinely asynchronous and the page previously only ever fetched once.
- Backend integration tests hardcoded FluxVM's real default port (7788)
  for the test driver, so a host that also runs a real, auth-enabled
  FluxVM instance there got a genuine 401 instead of the expected
  connection-refused. Now uses an OS-assigned ephemeral port.
- **agent-runtime**: session creation held a single *global* lock across
  the entire sandbox resume/create flow, including the outbound FluxVM
  HTTP call. A hung FluxVM call permanently blocked session creation for
  every agent until the process was restarted. The lock is now scoped
  per agent name, and the FluxVM call is bounded by a 120s
  `tokio::time::timeout`, so a hang becomes a bounded, lock-releasing
  error instead of blocking forever. Verified live: a genuinely-hung
  attempt was cut off at exactly 120001ms with the service staying
  responsive throughout.

### Security
- The daemon secrets manager now encrypts values at rest with AES-256-GCM
  (random nonce per value, secret id bound as associated data) instead of a
  hard-coded XOR key. Set `ZYVOR_SECRETS_KEY` to a base64 32-byte key to keep one
  key across restarts; an invalid value stops startup. Without it, each process
  uses a random key, and the store is in-memory only.
- Bumped `rustls` 0.23.44 → 0.23.45, fixing RUSTSEC-2026-0285 (rustls
  accepted TLS 1.3 handshake messages sent at the wrong encryption level
  when packed into the same record as a key-changing message — the
  handshake transcript stays authenticated, so this couldn't be used to
  alter or complete a handshake, but a peer's plaintext messages weren't
  being rejected the way RFC 8446 §5.1 requires). Found blocking `main`'s
  `cargo-deny` gate while checking release readiness — newly published,
  unrelated to any other change in this release. `cargo deny check
  advisories` clean afterward.
- Removed `web-legacy/`, the pre-consolidation dashboard superseded in
  May 2026 and unreferenced by any build/CI since — resolved ~20 open
  Dependabot alerts in dead code rather than patching it.
- Patched the unmaintained `users` crate (pulled in transitively via
  `pam`) to `uzers`, its actively maintained fork, via a small local
  shim crate (`backend/vendor/users-shim`).
- Upgraded `website/`'s vulnerable transitive deps (`qs`, `uuid`,
  `serialize-javascript`, `image-size`) via npm `overrides`, and
  `integrations/machina`'s `serde_with`. `image-size` (pulled in
  transitively via `@docusaurus/mdx-loader`, used only at site build time
  against our own docs content) had two high-severity infinite-loop DoS
  advisories (GHSA-w3rx-r6r6-pgpr, GHSA-5p2g-fcmc-qvqq) against `<= 2.0.2`;
  fixed upstream in `2.0.4`, published after the advisories despite
  Dependabot not yet showing a `first_patched_version` for either.
- `integrations/machina/desktop/src-tauri`'s `glib` (0.18.5, pulled in
  transitively via `gtk`/`webkit2gtk`/`wry` for the Linux Tauri build) has
  an open medium-severity soundness advisory (GHSA-wrw7-89jp-8q8g,
  `VariantStrIter`'s `Iterator`/`DoubleEndedIterator` impls) fixed in
  `glib 0.20.0` — left unpatched for now: Tauri 2.11.5 (already the latest
  published 2.x release) and the whole gtk-rs stack it pulls in are still
  on the 0.18.x generation, and the `gtk` crate itself hasn't published a
  release past 0.19.0 yet, so there is no compatible upgrade path without
  either a breaking gtk-rs major-version jump across a dozen crates or
  waiting on upstream Tauri/wry/tao/muda to move first.
- **Agent Runtime** — standalone component (`agent-runtime/`, `sdk/agent-runtime/`)
  for deploying durable TypeScript agent sessions, each in its own FluxVM
  sandbox: immutable content-addressed agent deployments, durable event
  journal with resumable SSE, mid-run steering/cancellation, checkpoint+pause
  hibernation, a host-side egress broker that injects provider credentials
  only after a request leaves the guest, idempotent session creation
  (`request_id`), per-agent concurrency caps, safe waiting-only
  auto-hibernation, method/path/port credential scopes, and single-use
  prewarmed FluxVM pools for low-latency starts. See
  [agent-runtime/README.md](agent-runtime/README.md).
- **Container Groups** — Kubernetes-style Pod-group workload backed by
  FluxVM Secure Containers: tenant scoping + audit trail, image pull
  secrets, per-tenant namespace isolation, liveness/readiness probes,
  Kubernetes NetworkPolicy, quota/billing wiring, hostPath backup/restore,
  CRD/Helm packaging, `zyvorctl` CLI commands, and a web UI page. See
  [docs/container-groups.md](docs/container-groups.md).
- **Service Fabric remote-backend lifecycle v2** — weighted drain handoff
  (`POST …/remote-backends/…/drain`), optional `vip` multi-VIP Maglev match,
  inject Ready + active Draining remotes (expired drains skipped),
  `list_services` reconcile path; Geneve/VXLAN tunnels still N/A.
- **Service Fabric full mesh datapath v1 (remote backends)** — durable
  `RemoteBackend` catalog + `/api/dataplane/remote-backends` CRUD/reconcile;
  merges same-domain Ready peer backends into FluxVM Maglev service upserts
  on owning-domain nodes (local backends preserved; tunnels still N/A).
- **Service Fabric remote identity directory (minimal ClusterMesh)** — durable
  `RemoteIdentity` catalog + `/api/dataplane/remote-identities` CRUD/reconcile;
  fans CIDRs into FluxVM `POST /v1/network/ipcache/remote` on owning-domain nodes;
  policy apply can soft-merge same-domain remote IDs before fan-out.
- **Service Fabric multi-site fencing** — optional `site_id` / `route_domain` on service
  intent and edge leases; anycast advertise + policy fan-out scoped to owning domain.
- **Service Fabric pressure proxy** — `POST /api/dataplane/services/pressure/reconcile`.
- **Service Fabric v6** — transactional multi-node service identity/L7 policy APIs proxied to FluxVM.

### Removed
- **machinectl / systemd-machined surface** — deleted `/api/machines` and the
  Machines UI (`/app/machines`); systemd unit no longer waits on
  `systemd-machined`; deploy/selftest/ctl checks dropped; migration target
  start uses `zyvorctl` (not `machinectl`); HA Level-2 machinectl fence
  removed. Use FluxVM-backed **Virtual Machines** (`/app/vms`, `/api/vms`).

### Added
- FluxVM CEP endpoints + MicroVM metrics in Fabric: `GET /api/dataplane/endpoints`
  (`identity_source`), Edge Dataplane **Endpoints** tab,
  `GET /api/dataplane/microvm-metrics`, Prometheus job `fluxvm-microvm` (`:9108`),
  `zyvorctl dataplane endpoints`, and ROADMAP sync with FluxVM Phase 2b/3c.
- Observe-all ops pack: `dataplane-follow` / `doctor` / `bundle` / `timers` /
  `chaos-failclosed` scripts, GitOps + Terraform dataplane examples, and
  `policy_control` helpers (management lockout, fingerprint, flow filter,
  Guard timers) ([docs/dataplane-observe-all.md](docs/dataplane-observe-all.md)).
- Observe pack on `policy_control`: explain, dry-run Guard, templates
  (open/guard/web/dns-only/no-world), drop-reason catalog;
  `GET …/dataplane/explain`, `GET …/dry-run`, `GET /api/dataplane/templates`,
  `zyvorctl dataplane explain|dry-run`
  ([docs/dataplane-observe-pack.md](docs/dataplane-observe-pack.md)).
- Per-VM Cilium-style packet-flow controls: Guard / Audit / Open / Invert /
  Block / Allow in the VM Dataplane Policy tab, Block-from-flow in the Flows
  view, `POST /api/vms/{name}/dataplane/policy/control`, and
  `zyvorctl dataplane policy guard|audit|open|invert|block|allow`.
- Hubble-style packet flow in the Fabric console: Edge Dataplane **Packet flow**
  tab and VM Dataplane Flows view with Colorful / Normal themes, hop path
  (guest → tap → tc/eBPF → uplink → peer), `GET /api/dataplane/hubble/flows`,
  and `zyvorctl dataplane hubble --style color|plain|json` (`--style` avoids
  clashing with global `-o/--output` table|json|yaml).
- DevOps pack: probe contract with FluxVM (`docs/contracts/fabric-fluxvm-readyz.json`), `scripts/devops-gate.sh`, GitHub/GitLab/GitOps/Terraform/Ansible examples under `examples/devops/`, and `docs/DEVOPS.md`.
- Proven-infra pack for issues #14–#17: `benchmarks/` harness (health / readyz / inventory / concurrent p50/p99), chaos qualification script, `scripts/upgrade-rollback.sh` N→N+1 snapshot/rollback/verify, and docs under `docs/proven-infra/` (compatibility matrix, SLOs, chaos, upgrade).
- `backup` crate is a workspace member with create/restore/delete and corrupt-archive fail-closed tests.
- Quorum majority tests in `fault-tolerance` and heartbeat window tests in `ha`.
- Lab verify scripts: `scripts/test-proven-infra.sh`, `scripts/test-lab-verify.sh`
  (devops units + live gate + proven-infra suites + edge dataplane e2e with lab
  auth fallbacks; stdin closed for SSH-safe runs).
- DevOps gate TLS: `scripts/devops-gate.sh` uses `curl -k` and auto-picks Fabric
  HTTPS then HTTP when `FABRIC_URL` is unset; `make test` runs `test-devops`.

### Changed
- Docs refreshed for JWT `tenant` claim enforcement, `driver.fluxvm_token` when
  FluxVM auth is on, compose `/readyz` healthchecks, and optional
  `network.hubble_ui_url` ([hubble-ui.md](docs/guides/operations/hubble-ui.md)).
- Docs and tutorials refreshed for `/readyz`, VM `tenant`, and FluxVM production
  alignment (first-VM, security, edge-dataplane series, install/k8s/docker,
  monitoring, API reference, billing).
- Drop remaining `ssahani/` GitHub and Terraform Registry namespaces in favor of `zyvorai/` (`zyvorai/fabric`, `zyvorai/zyvor-fabricd`).
- Docs refreshed for FluxVM Network Fabric **schema v4** edge dataplane (groups,
  CNP, effective, health/ipcache/FQDN) across operator guides, user pages, and
  tutorials ([09-edge-dataplane.md](docs/tutorials/09-edge-dataplane.md),
  [edge-dataplane/](docs/tutorials/edge-dataplane/README.md)).

### Added
- Edge Dataplane **Open Hubble** button when `network.hubble_ui_url` is set;
  [ROADMAP-DENSITY.md](docs/ROADMAP-DENSITY.md) for Cilium CEP / density phases.
- Project-production alignment with FluxVM: unauthenticated `GET /readyz`
  (store + FluxVM `/readyz`), VM `tenant` on create + `GET /api/vms?tenant=`,
  FluxVM client `tenant` / `readyz` / `list_vms_by_tenant`, label→tenant
  inheritance on VM start.
- JWT `tenant` claim (from user DB) with FluxVM-style create/list/get/mutate
  scoping; `network.hubble_ui_url` surfaced via `/api/capabilities`.
- `zyvorctl create --tenant`, Create VM UI tenant field, fabric-doctor
  `/readyz` checks (`--fabric-ready-url` / `--fluxvm-ready-url`), and k8s
  readiness probes (`fabricd` → `/readyz`; FluxVM → `/healthz` + `/readyz`).
- Docker Compose healthchecks require both `/health` and `/readyz`.
- Fabric proxy of FluxVM schema v4: `/api/dataplane/*`, VM `…/dataplane/effective`,
  Edge Dataplane console (`/app/edge-dataplane`), `zyvorctl dataplane` group/cnp/…
  commands, and `scripts/test-edge-dataplane-e2e.sh`.
- Tutorial 08: drive Fabric with OpenStack clients (`docs/tutorials/08-openstack-clients.md`) — Keystone token, Nova/Glance/Neutron/Cinder via `openstack` CLI and curl, public URL setup, Terraform/Ansible outline.
- `openstack-compat` crate and `/identity` `/compute` `/image` `/network` `/volume` routes: experimental OpenStack wire-protocol façade (Keystone/Nova/Glance/Neutron/Cinder) on the same daemon port as Fabric. Catalog URLs come from `daemon.public_url` / `ZYVOR_FABRICD_PUBLIC_URL` (or listen + TLS). See [docs/openstack-compat.md](docs/openstack-compat.md).
- `host-lifecycle` crate: deterministic host maintenance evacuation planner and async job manager — preflight blockers, capacity-aware target selection, live/cold migration policy, bounded-parallel execution, and failure semantics that leave a partially evacuated host cordoned rather than guessing. Not yet wired into the scheduler or server routes (see [docs/host-lifecycle.md](docs/host-lifecycle.md) for the intended follow-up integration).
- `enterprise-identity` crate and `/api/identity/scim/*` + `/scim/v2/*` endpoints: SCIM 2.0 lifecycle provisioning and group-to-role sync for Entra ID / Okta on top of Fabric's existing OIDC/SAML/LDAP auth providers. Dedicated, hashed, constant-time-compared provisioning bearer tokens; deprovisioning takes effect on next login. See [docs/scim-identity.md](docs/scim-identity.md).
- Redesigned the sign-in page (`/login`) with the Zyvor Z mark and Apple-style visual polish (depth, spacing, focus states).

### Fixed
- `jsonwebtoken` 11 login panics: enable the `aws_lc_rs` crypto backend so Fabric JWT encode/decode works after the Dependabot bump.
- Running-VM snapshots ignored `snapshot_type`: both Disk and Full used QMP `snapshot-save` (memory dump), so UI "Disk Only" still timed out under load / the 60s HTTP layer. Disk now uses `blockdev-snapshot-internal-sync`; Full keeps `snapshot-save` with a 300s poll budget; HTTP timeout raised to 330s; Snapshots tab defaults to Disk.
- Live snapshot create now waits/retries for QMP readiness (409 when still starting); UI retries on 409; Snapshot Manager gained Disk/Full picker; FluxVM HTTP client timeout raised to 180s; auto-healer skips VMs updated within 90s to avoid restart storms after start.
- Workspace-wide clippy lint drift across ~24 crates that had accumulated under current stable Rust (mostly `new_without_default`, `derivable_impls`, and small iterator/idiom lints) — `cargo clippy -- -D warnings` is green again.
- `fault-tolerance`'s test-only `MockDriver` was missing `get_cgroup_path`, a method `driver-core::VMDriver` gained since the mock was last updated — a real compile error in test code, not just a lint.
- Agent Runtime cold-start race: the guest's vsock channel isn't up the instant `create_sandbox()` returns, so the first guest-agent call after a cold create routinely failed with "connecting to vsock proxy socket ... No such file or directory." Found by deploying against a real FluxVM host with real KVM — nothing in CI exercises this path yet. Now retries until the channel comes up or `guest_start_timeout_secs` elapses.
- `fabric-agent deploy` (the actual CLI, not just its CI smoke build) couldn't resolve `@zyvor/fabric-agent` in an agent's own `import` — the package isn't published to npm yet, so every real deploy failed. `esbuild`'s `alias` option now points at the SDK's own local source.
- `api-audit` CI smoke test: TLS defaults to enabled with cert/key paths under root-owned `/etc/zyvor-fabricd/tls`, and self-signed cert generation is fatal on startup — the smoke script never disabled it even though it only ever talks plain HTTP. `[tls] enabled = false` in the generated config.
- 8 pre-existing e2e failures (VM clone, VM delete, bridge creation): the FluxVM HTTP stub's generic response didn't match the shape `fluxvm-client` deserializes, clone needs a real disk somewhere findable, and bridge creation needs `CAP_NET_ADMIN` the daemon process didn't have in CI. All reproduced live on a real host before fixing; `e2e` is fully green (153/153) for the first time.

## 0.2.1

### Fixed
- Fixed a mislabeled command in the installation guide (`# Using zyvorctl` headed a `zyvor-fabricd-ctl` example — a different binary).

### Added
- Real test coverage for the Kubernetes operator (`operator/`), previously zero: serde default-fallback behavior, error formatting, and the `reconcile()` loop itself end-to-end against a mocked fabric API and a mocked Kubernetes API client.

## 0.2.0

### Added
- Docker/Podman deployment support — the existing Dockerfile/compose now actually work, wired up against FluxVM.
- Hybrid Apple-style web UX (marketing + `/app` console), replacing the terminal UI (`zyvorctl-tui` removed).
- Collapsible, icons-only sidebar with per-viewer persistence.

### Fixed
- Memory limit/usage endpoints (`PUT`/`GET /api/vms/:name/memory/{limit,usage}`) 404'd on every real VM — they looked up cgroups by VM name, a convention FluxVM's UUID-keyed cgroups never match. Now resolved through the driver's real cgroup path.
- Memory, disk, and NIC hotplug could fail with "Device not found" on a fresh QMP reconnect between `object-add`/`blockdev-add` and the following `device_add`.
- Snapshot creation and the autoscaler's CPU/memory hotplug path reconnected to the QMP monitor on every single call — under contention this could wedge the monitor for both the request itself and unrelated connections.
- QMP's read timeout (10s) was too short for `snapshot-save`'s vmstate dump, which can legitimately run well past that under disk contention; raised to 300s.
- VNC canvas silently rendering at 0x0.
- WebSocket console-open failures were swallowed instead of surfacing to the browser.
- `generate-page-index.mjs` had regressed the `/app` route prefix and marketing section.
- A dead link and a wrong brand mark in the user guide index.
- Stale `vmspawnd`/`vmctl-tui` references and a wrong Ansible API port in docs.
- zyvorctl CLI examples throughout the user feature guide and README used a nonexistent `zyvorctl vm <subcommand>` pattern, a `--name` flag, and a `4G` memory suffix — none of which the real CLI supports. Corrected to match the actual flat command surface.
- GPU passthrough docs described vGPU/Intel GVT-g support, GPU-specific REST endpoints, and a `zyvorctl gpu` CLI subcommand — none of which exist. Rewritten to describe the real capability: generic PCI/VFIO passthrough.
- Fictional etcd-clustering and memory-based live-migration content removed from docs.

### Changed
- Renamed the Ephemera VM driver integration to FluxVM (crates, config keys, docs, scripts).
- Relicensed to Apache License 2.0; removed proprietary legal docs and audited dependency licenses.
- Internal `vmspawnd_*` identifiers renamed; fictional Prometheus metrics corrected or flagged.

## 0.1.0

- Initial release.
