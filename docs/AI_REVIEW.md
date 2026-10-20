# AI code review (fabric)

_Generated 2026-10-20 by claude-opus-5-5 from a snapshot of tracked sources; verify before acting._

# Code review: zyvor-fabric

## Summary

Zyvor Fabric is a Rust private-cloud control plane (`zyvor-fabricd`, 53 crates). It drives VMs through a separate FluxVM engine and is reached through a CLI, a React console, a Kubernetes operator and a Terraform provider. The repo also contains **Keep**, an agent runtime that runs untrusted agents in FluxVM microVMs with signed policy, a host-side credential vault and phone-signed approvals. It ships around 80 example "packs" plus Mac and iOS clients.

The material reviewed is mostly documentation and config. Source code was largely not visible, so most findings are doc/config correctness issues.

## Bugs and correctness risks

**Quickstart and clone commands use the wrong directory.**
- `README.md` runs `git clone https://github.com/zyvorai/zyvorai-fabric.git && cd fabric`. The clone creates `zyvorai-fabric`, so `cd fabric` fails.
- `docs/development/README.md` and `CONTRIBUTING.md` clone `fabric.git`, then `cd zyvor-fabric`. That directory will not exist.

**The repository identity is inconsistent.** These references are mixed across files:
- `zyvorai/fabric` (`ARCHITECTURE.md`, `SECURITY.md`, `docs/keep/README.md`)
- `zyvorai/zyvorai-fabric` (`README.md`, `website/README.md`)
- `zyvorai/fluxvm` vs `zyvorai/zyvor-fluxvm` for FluxVM

**Port and path contract drift.**
- `docs/guides/cli/README.md` and `docs/reference/api/README.md` use `http://localhost:3000/api`. Everything else uses `:9095`.
- `docs/quick-reference/README.md` says the API base is `/api/v1/` and the console is `/api/v1/ws/{vm_name}/console`. `docs/reference/api/README.md` gives `/api/vms/:name/console`.
- Most curl examples use `/api/...` without `v1`.
- `ansible/README.md` says the collection targets `/api/v1/`.

**HTTP vs HTTPS defaults conflict.**
- `docs/guides/operations/README.md` calls `/health` with `-sk` over HTTPS. It then calls `https://127.0.0.1:9095/api/auth/login` without `-k`, which fails against a self-signed cert.
- The operator, Terraform and Ansible docs default to `http://127.0.0.1:9095`.

**The systemd requirement contradicts itself.**
- "No systemd dependency" appears in `docs/user/README.md`, `docs/guides/operations/README.md` and `docs/guides/decision-support/README.md`.
- `docs/getting-started/README.md` requires "systemd 256 or later", and `docs/tutorials/README.md` requires "systemd v260+".

**The production compose file has diverged from the eval one** (`docker-compose.prod.example.yml` vs `docker-compose.yml`). The prod `fluxvm` service lacks:
- the `/sys/fs/bpf` mount,
- the `memlock` ulimit,
- `SYS_RESOURCE`,
- the `--config /etc/fluxvm.toml` dataplane config.

Edge eBPF confinement, which Keep relies on (`deny_udp` and so on), likely won't attach in "prod". That is a silent downgrade.

**Goal planner example is likely wrong.** `examples/keep-agents/goal-planner/README.md` POSTs JSON to `/v1/goals` without a `content-type: application/json` header. It also uses `"agent":"mail-compose"` as the worker for a "weekend in Lisbon" plan. This is likely rejected or misleading.

**The SDK samples use two client names.**
- `sdk/agent-runtime/README.md` uses `keep.` in one section and `fabric.` (`fabric.sessions.createMany`, `fabric.agents.warmPool`) in others.
- No `fabric` instantiation is shown, so copy-paste fails.

**Wrong path and duplicated entries in the development guide.** `docs/development/README.md` says `AppState` is defined in `Zyvor Fabric/src/server.rs`. Presumably it is `zyvor-fabricd/src/server.rs`. The tree also lists `driver-core` and `fluxvm-driver` twice.

**Versions and names are inconsistent.**
- `sdk/python/fabricctl/__init__.py` reports `0.1.0`. `backend/crates/input-guard/Cargo.toml` hardcodes `0.3.0` instead of using `version.workspace`.
- `examples/keep-agents/invoice-model-brief/README.md` is titled "invoice-brief".

## Security concerns

**Eval compose exposes a known password.** `docker-compose.yml` binds `0.0.0.0` on host networking with a fixed default admin password (`eval-admin-only`). It also grants `SYS_ADMIN`, `SYS_PTRACE` and `NET_ADMIN`. Anyone on the LAN gets admin unless the operator overrides it. Consider defaulting to `127.0.0.1` for eval.

**`deny.toml` silences advisories.**
- It ignores 8 RUSTSEC advisories (including `RUSTSEC-2023-0040`, `RUSTSEC-2026-0194/0195`) with no per-ID justification or expiry.
- `unknown-git = "warn"` and `wildcards = "allow"` weaken supply-chain gating.

**Bearer tokens travel in URLs.** Query strings end up in proxy and access logs. Examples:
- `WS /ws/sessions/{id}/browser/screencast?token=`
- `/keep/cockpit?session=` ("token in query or form")
- the screencast `?token=` options in `docs/keep/cockpit/README.md` and `docs/keep/browser/README.md`

**Secrets leak through shell history and environment variables.**
- `integrations/machina/client/README.md` exports the admin password into the environment.
- Tutorials put passwords in `curl -d`.
- The operator README passes the admin webhook token via `helm --set`, so it lands in shell history and release values.

**Packs can reach host loopback through the broker.** `examples/keep-agents/deploy-op/keep.policy.yaml` allows `127.0.0.1` GET with `ask: never`, and `migration-op` and `infra-ops` target `https://127.0.0.1:9095`. Because the egress broker runs on the host, this gives a sandboxed agent a path to host-loopback services. The Sentinel risk check lists `private_ip_host` as High. It is unclear from the docs whether loopback triggers that check.

**The vendor gateway is a reference design, not production-ready.** `reference/vendor-gateway` trusts the `acr: "strong"` claim for device enrolment. Its HS256 mode pins no issuer or audience, and its state lives in a JSON file. The authors acknowledge this, but it should be clearly fenced from production use.

**Binary pinning has two acknowledged gaps** (`docs/keep/sentinel`):
- Pins are trust-on-first-use, so the first binary seen wins.
- Attribution trusts the guest kernel.

**Approval receipts are not at-most-once on a crash.** If the runtime dies between the send and the write, no receipt is recorded (`docs/keep/receipts`). The docs say this; it is worth tracking.

**Supported versions are stale.** `SECURITY.md` lists `0.2.x` as supported, but the current release is `0.3.0` (README and `docs/social`). Taken literally, the current release is unsupported.

## Test and CI gaps

**Many features are verified only against fakes, by the docs' own admission:**
- the Microsoft Graph connectors (never run against real Microsoft),
- Google approved-send and event creation,
- HashiCorp Vault sources (mock only),
- the `zyvor-keep` Helm chart (never installed),
- the iOS app (never run),
- `keep-up.sh --install-fluxvm`,
- the GPU cells,
- the OCSF export (not schema-validated).

**Several example samples were never checked against real exports.** The Windows, VS Code and bookmarks packs say "not checked against a real export".

**Live-path CI depends on hardware.**
- The live CI job needs a self-hosted KVM runner (`examples/devops`).
- All four pilot runs (`docs/keep/pilot-runs/`) are from one day.
- Their `SUMMARY.md` files reference `happy/` and `deny/` directories not visible here.

**The seccomp filter is x86_64 only.** An arm64 `strict` cell does not start (`docs/keep/cell`). There is no indication arm64 is covered in CI.

**Ansible tests probably don't run as written.** `ansible/README.md` advertises `ansible-test units/integration`, but there is no `galaxy.yml` or collection layout. `ansible-test` requires one.

**`build.sh` uses `npm install`, not `npm ci`.** Builds are not lockfile-reproducible.

## Documentation gaps

- The pack index in `examples/keep-agents/README.md` lists about 17 packs. More than 60 exist; most banking, OCR, Mac and Windows packs are missing.
- There is no single canonical statement of:
  - API prefix (`/api` vs `/api/v1`),
  - default scheme and port,
  - repo URL.

  `docs/API-VERSIONING.md` and `docs/NAMING.md` are referenced but were not visible to confirm.
- Marketing figures (780+ endpoints, 92 pages, 31-round audit) are repeated across many files. They will drift as the code changes.
- The production compose file has no explanation for dropping the dataplane config.
- Keep's honesty caveats (`software-test` evidence class) are thorough. Core Fabric docs give no equivalent "verified vs claimed" status for DRS, FT, site recovery or distributed storage.

## Top 5 recommended actions

1. **Fix the onboarding commands.** Correct the clone and `cd` commands in `README.md`, `CONTRIBUTING.md` and `docs/development/README.md`. Unify the repo and FluxVM URLs.
2. **Harden the compose files.**
   - Bind `docker-compose.yml` to `127.0.0.1` by default and require an explicit admin password.
   - Restore the BPF mount, memlock ulimit and dataplane config in `docker-compose.prod.example.yml`, or document why they were dropped.
3. **Publish one API/port/TLS contract and sweep the docs to match.** Cover `/api` vs `/api/v1`, `:9095` vs `:3000`, and HTTPS defaults, including the CLI guide, the API reference, the operator and Terraform defaults, and the systemd prerequisites.
4. **Tighten supply-chain and security policy.**
   - Annotate each ignored RUSTSEC ID in `deny.toml` with a reason and owner.
   - Set `unknown-git = "deny"`.
   - Update `SECURITY.md` supported versions to `0.3.x`.
   - Deprecate query-string tokens in favour of headers or subprotocols.
5. **Close the loopback-egress question.** Confirm and test whether Sentinel's risk check flags `127.0.0.1` allow rules like `deploy-op`'s. Add a CI test that a pack cannot reach arbitrary host-loopback ports via the broker.
