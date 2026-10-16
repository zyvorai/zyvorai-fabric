# Zyvor Fabric Agent Runtime

Deploy JavaScript/TypeScript agents like serverless functions while giving every session its own durable FluxVM Linux sandbox.

## Capabilities

- immutable, content-addressed agent deployments
- one FluxVM sandbox per session
- durable JSONL event journal with monotonically increasing sequence numbers
- resumable SSE streams (`?after=<seq>`)
- mid-run steering and cancellation
- memory+disk checkpoint followed by pause for hibernation, then resume
- host-side egress broker: provider API keys are resolved from host environment variables and injected only after the request leaves the guest
- arbitrary credential headers (`Authorization`, `x-api-key`, etc.)
- per-agent host allowlists and credential grants
- private/link-local egress blocked by default to reduce SSRF/metadata exposure
- TypeScript/JavaScript SDK and `fabric-agent deploy` CLI
- no dependency on a particular model SDK or provider
- idempotent session creation with caller `request_id` keys
- per-agent non-terminal session concurrency caps
- automatic hibernation only at the safe `ctx.nextSteer()` waiting point
- method/path/port scopes on host-side credentials
- bounded-concurrency SDK fan-out with stable result ordering
- single-use prewarmed FluxVM sandbox pools per immutable agent version
- warm-pool health repair, stale-claim recovery and automatic replenishment
- runtime-owned TTL expiry so warm sessions receive their full requested lifetime
- `start_mode` + `startup_ms` observability for warm/cold launch measurement
- per-session operation serialization for steer/hibernate/resume/cancel/delete/expiry
- a coding-agent harness for `claude`, `codex`, or `gemini`, with operator approvals
- agent-to-agent delegation that keeps the child's own allowlist and grants
- cron schedules, HMAC webhooks, and bounded loops
- an MCP endpoint for listing agents, listing executions, and chatting
- GitHub session CI that runs those paths with no FluxVM and no model key
- **Keep** product surface (0.1 live proof): Keep mode fail-closed signed
  `keep.policy.yaml`, BYO `model_socket`, credential `authorize_resolve`, session
  cockpit + browser live view, measured/`software-test` labeling, `scripts/merionctl`,
  scoped export tokens, `./scripts/merion-live-lab.sh` —
  [docs/merion/MERION.md](../docs/merion/MERION.md) · [PRODUCTION.md](../docs/merion/PRODUCTION.md) ·
  [Tutorial 16](../docs/tutorials/16-merion-workstation.md) · [CI](../.github/workflows/keep.yml)

The runtime is intentionally a standalone component in the Fabric repository. It consumes FluxVM's existing `/v1/sandboxes` API directly and does not alter the existing `zyvor-fabricd` VM API or backend workspace.

## Architecture

```text
agent.ts -> fabric-agent deploy -> immutable bundle
                                 |
SDK / HTTP -> Agent Runtime -----+---- durable session/event state
                  |              |
                  |              +---- host credential descriptors -> host env
                  |
                  +---- FluxVM /v1/sandboxes -> one Linux sandbox/session
                                      |
                                   worker.mjs
                                      |
                              session capability only
                                      v
                              host egress broker
                                      |
                      inject provider key after VM boundary
                                      v
                       OpenAI / Anthropic / any HTTPS API
```

Provider secret values are never serialized into agent deployments, session records, guest files, environment variables, or API responses. The guest receives only a random session-scoped egress capability. The broker validates the session, immutable agent version, destination host, credential grant and credential host before resolving the provider secret from the host environment.

## Host requirements

1. A working FluxVM server reachable from the Agent Runtime.
2. A FluxVM sandbox template containing Node.js 20+. A `claude`, `codex`, or `gemini` agent also needs that CLI on `PATH` in the same template. The harness adapter is still Node; it does not put provider keys in the guest.
3. The template should use `tap` + `netns` networking. The worker derives the host-side gateway from its default route so it can reach the egress broker. If that is not appropriate, set `ZYVOR_AGENT_EGRESS_ADVERTISE_HOST` explicitly.
4. The FluxVM guest agent must be present. FluxVM sandbox creation enables it by default.

Example FluxVM template name used below: `node22-agent`.

**Known limitation:** `POST /v1/sandboxes` always creates a FluxVM
`flux-vm`-backend sandbox (its own in-tree microVM hypervisor,
`fluxvm-hypervisor`), regardless of what a template's `backend` field
says. This backend is less mature than FluxVM's QEMU backend for
general-purpose images: confirmed live (direct `fluxvm-hypervisor`
invocation with serial output captured) that it hangs indefinitely
bringing up a second vCPU (`--cpus 1` boots fine; `--cpus 2+` doesn't),
and separately hangs during guest kernel boot at virtio-mmio device
probe on at least one general-purpose Ubuntu+Node.js image. Until
that's resolved, build and test your template image with `--cpus 1`
first before assuming a real session will complete end to end.

## Coding-agent harness

Set `manifest.runtime` to `claude`, `codex`, or `gemini` to run that CLI inside the session instead of a JavaScript bundle. The choice is part of the immutable version. Deploy an existing `CLAUDE.md` or `AGENTS.md` without rewriting it:

```bash
fabric-agent deploy ./CLAUDE.md --name reviewer --template node22-agent \
  --runtime claude --credential anthropic --allow-host api.anthropic.com
```

The template must contain Node.js and the selected CLI. The adapter writes the instruction file into the sandbox and points `ANTHROPIC_BASE_URL`, `OPENAI_BASE_URL`, and `GEMINI_API_BASE_URL` at a loopback shim. The shim calls the host egress broker, which injects the granted credential. Placeholder client keys in the guest are not provider secrets. If the broker call fails, the shim responds `502` with `upstream request failed` and writes the exception to the guest process log. It does not put the exception message or stack in the HTTP body.

A JSON bundle is also accepted: `{"instructions":"...","files":{"CLAUDE.md":"...","src/main.py":"..."}}`.

When the CLI prints `ZYVOR_APPROVAL <question>`, the runtime opens an operator approval and waits. `POST /v1/approvals/{id}` with `{"decision":"approved"}` or `"denied"` steers the session. `POST /v1/sessions/{id}/delegate` starts another agent; the child keeps its own allowlist and credential grants, and delegation stops at three levels.

## Schedules, webhooks, and loops

- `POST /v1/schedules` with a five-field UTC cron expression admits a session each time it is due.
- `POST /v1/webhooks` returns an HMAC secret once. Callers `POST /v1/hooks/{id}` with `X-Zyvor-Signature: sha256=<hex>` over the raw body. That route does not use the API bearer token.
- `POST /v1/loops` repeats a session until `max_runs`, `max_duration_secs`, `max_cost`, or `max_no_progress`. A loop must set one of those.

## MCP

`POST /mcp` speaks MCP over JSON-RPC (`initialize`, `tools/list`, `tools/call`) on the same bearer token as the rest of the API.

- `list_agents`
- `list_executions`
- `chat_with_agent` creates a session, or steers one when `session_id` is set

## Fabric-managed inference (Phase 6)

Agents can call a Fabric `InferenceEndpoint` Maglev VIP without an external
provider API key. Add a credential with `"kind": "fabric"` (see
`credentials.example.json`), grant it on the agent, set
`ZYVOR_FABRIC_INFERENCE_BASE` to the VIP base URL, and optionally
`FABRIC_AI_API_KEY` when endpoint keys are enabled. HTTP to private Maglev
ports is allowed for `kind: fabric` only.

## Approvals and the action journal

An approval is a request for a human decision. `POST /v1/approvals` takes `session_id`, `prompt`, and optional `kind` (`custom` by default, `egress`, `purchase`, `send`), `subject` (a short target such as a host or recipient), and `planned_action` (structured JSON describing what will run if approved). The coding-agent harness still opens `custom` approvals itself when the CLI prints `ZYVOR_APPROVAL`.

Every approval and every brokered egress call is written to `audit.jsonl` in the state directory, one entry per line:

| Phase | Written when |
|-------|--------------|
| `planned` | An approval is opened |
| `approved` / `denied` | A human decides it |
| `performed` | An egress call completed |
| `denied` | An egress call was refused (allowlist, private network, scope, credential grant) |
| `failed` | An egress call was allowed but failed upstream |

Each entry commits to the previous entry's SHA-256, so editing or deleting a line breaks the chain. `GET /v1/audit` returns entries (newest last, `limit` defaults to 200, maximum 5000, optional `session_id`) plus `chain: {entries, chain_ok, broken_at?}`. Egress entries record the method, host, and path only, never the query string, headers, or credentials. Calls that fail the session capability check (401) are not journaled, because their claimed session id is unproven. A failed journal write is logged and does not fail the request.

Through the Fabric daemon these are `GET/POST /api/approvals`, `POST /api/approvals/{id}`, and `GET /api/audit/agent-actions`; from the CLI, `fabricctl approval list|approve|deny` and `fabricctl agent-audit`.

### Asking a human before egress

By default a request to a host outside `egress_allow_hosts` is refused (`"egress_mode": "deny"`). Set `"egress_mode": "ask"` in the manifest and the broker instead holds the request and opens an `egress` approval whose `subject` is the host and whose `planned_action` holds the method and URL without its query string. `egress_approval_timeout_seconds` (5-240, default 90) bounds the wait.

Decide it with `POST /v1/approvals/{id}` and `{"decision":"approved","scope":"once"}`. `once` (the default) releases the requests that were waiting; `session` also allows every later request to that host for the life of the session. Denying, timing out, or ending the session refuses the request with 403; a timed-out approval becomes `expired` and cannot be decided afterwards. Concurrent requests to the same host share one approval.

Approval only lifts the allowlist check. DNS pinning, the private-network gate, and credential host scoping still run afterwards, so approving a host never allows a request into loopback, private, or link-local ranges unless `allow_private_networks` is set. The agent only sees the 403 or the response: approving does not steer the session.

### Sentinel review

`"egress_mode": "sentinel"` puts a reviewer model in front of the operator. For a request to an unlisted host the broker asks an OpenAI-compatible chat endpoint for a verdict, and the reviewer sees only the agent name, its allowlist, and the request's method, host and path (never headers, bodies, query strings, or credentials). Configure it on the runtime:

| Variable | Meaning |
|---|---|
| `ZYVOR_AGENT_SENTINEL_URL` | Base URL; `/chat/completions` is appended. Point it at the Fabric inference gateway or any compatible server. |
| `ZYVOR_AGENT_SENTINEL_MODEL` | Model name. Required when the URL is set. |
| `ZYVOR_AGENT_SENTINEL_API_KEY` | Optional bearer token. |
| `ZYVOR_AGENT_SENTINEL_TIMEOUT_SECS` | Per-review timeout, default 15. |
| `ZYVOR_AGENT_SENTINEL_CAN_ALLOW` | `1` lets an `allow` verdict release a single request. |

The reviewer's authority is deliberately narrow, because the request it reads is written by a possibly injected agent. `deny` refuses the request with 403 and no human is asked. `escalate`, and any error, timeout, unparseable answer, or missing configuration, opens the normal `egress` approval with the reviewer's note in `planned_action.sentinel`. `allow` is treated as `escalate` unless `ZYVOR_AGENT_SENTINEL_CAN_ALLOW=1`, and even then it releases one request and never creates a session grant. Allow and deny verdicts are journaled as `sentinel.egress`. The private-network gate and DNS pinning still apply afterwards.

Changing `egress_mode` or the timeout changes the agent's version, like any manifest change; manifests that leave both at their defaults keep their existing version ids.

### GPUs

`"gpus": N` in the manifest (1 to 8) gives each cell N GPUs, passed through with VFIO. The runtime sends the count to FluxVM, which picks free ones itself, under a lock, so two sessions are never given the same GPU (see FluxVM's `docs/sandbox-gpus.md`).

```json
{ "template": "cuda-agent", "cell_backend": "qemu", "gpus": 1 }
```

- Deploy refuses combinations that cannot work: it needs `"cell_backend": "qemu"` (only QEMU cells can be given a device), and it cannot be combined with `confidential`, `warm_pool_size` or `idle_hibernate_seconds`.
- FluxVM must already have the GPUs bound to `vfio-pci` (`POST /v1/host/gpus/bind`); see `docs/gpu-passthrough.md`. A GPU in use by another VM is not picked.
- **No free GPU is a 503** on session create, with FluxVM's count of how many are free. There is no queue; the caller retries. The `503` message starts `no GPU is free for this agent`.
- **A CPU-only cell is never started by mistake.** After the create, the runtime reads the devices FluxVM assigned from the returned record. If there are fewer than the manifest asked for (an older FluxVM ignores `gpus`), the cell is deleted and the session fails with 502. The assigned addresses are journaled as `keep.cell.gpus`.
- The egress and credential policy are unchanged: a GPU cell still reaches the network only through the broker.
- Not verified on real GPUs: the runtime's part is tested against a stand-in FluxVM, and FluxVM's picking logic is tested on its own. A passed-through device sits outside what the runtime can attest or snapshot, and the cell's own workload sees the GPU directly.

### Egress guard

An operator-run HTTP service can refuse any request through the JSON broker, for every agent. It is the hook for a check the runtime does not have: a content filter, a per-tenant rule, a legal hold.

| Variable | Meaning |
|---|---|
| `ZYVOR_AGENT_GUARD_URL` | The guard's URL (`http` or `https`). Unset means no guard. |
| `ZYVOR_AGENT_GUARD_TOKEN` | Optional bearer token. Refused at startup unless the URL is `https` or a loopback host. |
| `ZYVOR_AGENT_GUARD_TIMEOUT_SECS` | 1 to 60, default 3. |

The guard is called after the agent's own rules (`egress_rules`, including body rules) and the DLP scan, and before an operator is asked or a credential is added, so it never sees a secret the runtime injects. It gets a JSON `POST`:

```json
{"version":1,"session_id":"…","agent":"desk","method":"POST","url":"https://api.example.com/v1/x",
 "host":"api.example.com","path":"/v1/x","header_names":["content-type"],"dlp":["aws-access-key"],
 "body_bytes":5,"body_sha256":"…","body_base64":"aGVsbG8="}
```

The URL has no query string or credentials, only header *names* are sent, and a body over 256 KiB is described by size and hash (`"body_omitted": true`). It answers `200` with `{"decision":"allow"}` or `{"decision":"deny","reason":"…"}`. The guard can only narrow what policy allows. It **fails closed**: a timeout, a connection error, any status but 200, an answer over 64 KiB, or anything but an exact `allow` or `deny` refuses the request with 403, and the agent sees the guard's reason (control characters removed, 200 characters at most). Refusals are journaled like any other denied egress.

It does not see CONNECT-proxy traffic, which exposes only `host:port`; put `egress_rules` on a host to keep it off the proxy.

## Containment

These controls are aimed at a compromised or prompt-injected agent. They stack: each one assumes the others may be bypassed.

### Network confinement

Every egress control above is moot if the sandbox can reach the internet itself. `"confinement": "strict"` (or `ZYVOR_AGENT_CONFINE=1` on the runtime, which forces it for every agent) applies a FluxVM per-VM eBPF policy before any agent code is written to the guest: only the host gateway, only on the broker and proxy ports; everything else, DNS included, is dropped. The runtime uses FluxVM's `POST /v1/vms/{id}/network/policy` and fails the session rather than run it unconfined if that call fails. The gateway must be an IP address (`ZYVOR_AGENT_EGRESS_ADVERTISE_HOST`, or the guest's default route). **Not yet verified against a live FluxVM:** whether the policy's port rules also match reply traffic on the tap, so try it on a real template and check that `curl --noproxy '*' https://example.com` fails while a brokered request succeeds.

### Approvals that reach a person

Set `ZYVOR_AGENT_APPROVAL_WEBHOOK` and `ZYVOR_AGENT_APPROVAL_WEBHOOK_SECRET` and every new approval is POSTed there, signed `x-zyvor-signature: sha256=<HMAC-SHA256 of the body>`, with the approval's kind, subject, prompt, `planned_action`, agent and `user_id` (so a receiver can route it to the right device) and the path to decide it. Delivery retries twice and is journaled as `approval.notify` if it finally fails; it never blocks the request. The same webhook also receives `run.finished` and `run.failed` when a one-click use case ends (`x-zyvor-event` names the event): the use case, session, artifact titles and connection count, never the file name or extracted text. A rejected upload or unknown use case (400/404) is not a failed run and sends nothing. The receiver answers approvals through the operator API (`POST /v1/approvals/{id}`). The agent cannot: the operator routes sit behind `ZYVOR_AGENT_API_TOKEN` on the public listener, and the broker and proxy listeners that the sandbox can reach serve nothing else (both are tested).

A credential descriptor can require a decision per request: `"requires_approval": ["POST"]` (methods, or `"*"`) and `"approval_kind": "send"` (default) or `"purchase"`. The broker then holds the request after every other check and opens an approval showing the method, the URL without its query string, the credential name, and the body's length and SHA-256, never the body or a header. Each request needs its own decision; nothing is remembered.

### Inside the sandbox: a contained worker

`"inner_container": "strict"` starts the worker (and so any browser it launches) through `/opt/zyvor/contain.sh`, which the runtime writes into the guest: an unprivileged `agent` user in a bubblewrap container with a read-only view of the guest (the agent cannot edit its own bundle, the worker or the launcher), a private `/tmp`, its writable home, no capabilities and no new privileges. The template needs `bubblewrap` and `util-linux` (`templates/browser-agent/build.json` has both), and it fails closed: without them the worker never starts. A seccomp filter (`/opt/zyvor/seccomp.bpf`, written next to the launcher and loaded with `bwrap --seccomp`) answers `EPERM` to the syscalls an agent has no business making: `ptrace`, `process_vm_*`, `mount` and the new mount API, `pivot_root`, `chroot`, module loading, `kexec`, `bpf`, `perf_event_open`, `userfaultfd`, kernel keyrings, clock and port changes, and `io_uring` (the list is `DENIED_SYSCALLS` in `src/contain.rs`). It is x86_64 only, kills other architectures and the x32 ABI, and the launcher refuses to run if the filter file is missing or the CPU is not x86_64. It leaves `unshare`, `clone`, `setns`, `memfd_create`, `seccomp` and `prctl` alone, because Chromium, V8 and Node use them. The VM stays the outer boundary; this limits what an escape from the agent's own process gets. It shares the network namespace on purpose, since the agent must reach the broker, so pair it with `confinement: strict`. Known gap: the guest agent's vsock channel is reachable from that namespace, so set a guest-agent token for an untrusted agent. There is deliberately no Landlock layer: see `docs/merion/REMAINING.md` §6 for why. Not exercised against a live FluxVM guest yet; the launcher's logic is tested with stand-in binaries. The filter itself was run under a real `bwrap` on a Linux x86_64 host (denied calls returned `EPERM`, ordinary process, socket and file work was unaffected, every syscall number matched the kernel header), but not through `contain.sh` in a cell, and not with Chromium.

### Always-on workstations

Deploy with `"persistent": true`, then declare a workstation: `PUT /v1/workstations/{agent}/{user_id}` (`GET`, `DELETE`, and `GET /v1/workstations` too; operator token only). The runtime keeps a session running for it and starts a new one when it ends, fails or expires, retrying with exponential backoff (5 s doubling to 5 min) while starts keep failing; the record shows `restarts`, `consecutive_failures`, `last_error` and `next_attempt_at`. State survives restarts through the user's per-user home volume, so it is meant to be used with `home_volume.per_user`. Deleting a workstation cancels its session and leaves the volume.

### Looking at the agent's browser

With `"browser_port": 9222` (Chromium started with remote debugging on that port, bound so FluxVM's sandbox proxy can reach it), an operator can:

- `GET /v1/sessions/{id}/browser/json/list` (also `json`, `json/version`, `json/protocol`) — raw DevTools listings with WebSocket/frontend URLs stripped
- `GET /v1/sessions/{id}/browser/view` — Keep 0.1 live tab summary (title + URL)
- `GET /keep/browser?session=<uuid>` — HTML that polls the live view

Only listing paths are forwarded, plus a screenshot and a read-only screencast through the host CDP bridge. **Input takeover is not implemented.**

### Confidential VMs, when the host has them

`"confidential": "auto"` asks FluxVM for a hardware-encrypted VM (AMD SEV-SNP or Intel TDX) and quietly runs a normal VM when the host cannot, recording why on the session (`confidential: {active, tech, reason}`, also in the `session.created` event). `"required"` refuses to run without one: FluxVM refuses the launch, and the runtime also deletes any sandbox that comes back without an active confidential status, including from a FluxVM too old to report one. Deploy rejects the combinations that would leak guest memory or disk: `warm_pool_size` and `idle_hibernate_seconds` (a snapshot copies memory out), and, for `required`, `home_volume` (a virtiofs share is readable by the host). `GET /v1/host/confidential` on FluxVM shows what a host offers.

**Not yet real on hardware.** FluxVM detects SEV-SNP and TDX but does not launch confidential guests yet (`LAUNCH_SUPPORTED` is false and guarded by a test): the QEMU arguments for them interact with FluxVM's memory-hotplug and CPU settings and cannot be checked without the hardware. So today `auto` always falls back and `required` always refuses, with an accurate reason. This is TEE isolation only: a user-held key that the operator cannot use is the key-broker work in `docs/design/confidential-agent-vms.md`, not done.

### Request rules, secret scanning and taint

- `egress_rules`: `[{"host": "api.example.com", "methods": ["POST"], "path_prefixes": ["/v1/messages"], "max_body_bytes": 65536}]`. A host with any rule needs a matching one; hosts without rules are unrestricted beyond the allowlist. Checked first, so a request the agent may never send never reaches an operator. The CONNECT proxy refuses a host that has rules, because a TLS tunnel hides the method and path.
- `"dlp": true` holds any brokered request whose URL, headers or body contain a secret-shaped string (private keys, AWS, GitHub, Slack and API-key formats, JWTs) for approval. Only the detector names are stored, never the match. This is pattern matching: it catches accidents and lazy exfiltration, not a determined encoder.
- `"taint": {"trusted_hosts": [...]}`: a session that reads a response (or opens a tunnel) from a host outside `trusted_hosts` is tainted (`tainted_by` on the session, journaled as `session.tainted`). While tainted, brokered writes (anything but GET/HEAD) need approval and Sentinel's `allow` is downgraded to an operator decision. Only an operator clears it: `POST /v1/sessions/{id}/untaint`. This is per session, not per process: the host cannot see processes inside the guest, and a tunnel is opaque, so it stops the classic "read a hostile page, then send a message" path but not a browser posting to a host it may already reach.

## Persistent home volume

`"home_volume": {"name": "research-home", "guest_path": "/home/agent"}` in the manifest mounts a FluxVM volume in the agent's sandbox. It is a host directory that outlives the sandbox, every session, and every new version of the agent: `name` defaults to the lowercased agent name, and `guest_path` defaults to `/home/agent`. Write state there and the next session sees it.

Requirements, all checked at deploy time:

- **A QEMU-backed template.** The volume is shared over virtiofs, which the in-tree `flux-vm` backend does not support. `template` must name a FluxVM template whose `spec.json` sets `"backend": "qemu"` (needs FluxVM with sandbox volumes, `feat/sandbox-volumes`, plus `virtiofsd` on the host). A sandbox on QEMU also avoids the 2+ vCPU hang of `flux-vm`, but it cannot be snapshotted.
- **`max_concurrent_sessions: 1`.** A volume attaches to one sandbox at a time; a second session that tries to start while the volume is attached gets `409`.
- **No `warm_pool_size` and no `idle_hibernate_seconds`.** Warm sandboxes are created before a session owns the volume, and QEMU sandboxes have no snapshot to hibernate to. A manual hibernate of such a session fails at FluxVM.

### One volume per user

For a fleet of per-user agent VMs, deploy one agent with `"home_volume": {"per_user": true}` and pass a `user_id` when creating a session (`POST /v1/sessions {"agent": "...", "user_id": "alice"}`, also accepted by schedules, loops and webhooks; delegated sessions inherit the parent's). Each user gets the volume `<name>-<user_id>`, at most one session per user runs at a time (a second returns `409`), and `max_concurrent_sessions` becomes an agent-wide cap instead of being forced to 1. `user_id` is 1-32 characters from `[a-z0-9._-]`, and is required for a per-user agent. `GET /v1/sessions?user_id=alice` lists one user's sessions.

The runtime trusts the `user_id` its authenticated API caller asserts. It is not an end-user identity check: put your own authentication in front and derive the id from it. `warm_pool_size` and `idle_hibernate_seconds` are still rejected.

## Sandbox size

`"resources": {"vcpus": 2, "memory_mib": 7900}` sets the size of each sandbox, passed to FluxVM on create (needs FluxVM `feat/sandbox-resources`; an older FluxVM ignores the fields, so check with `nproc` in a session). FluxVM treats the template's own `max_vcpus`/`max_memory_mib` as a ceiling and refuses a larger request. The runtime can add its own ceiling with `ZYVOR_AGENT_MAX_VCPUS` and `ZYVOR_AGENT_MAX_MEMORY_MIB`; a manifest above them is rejected at deploy. Volumes have no size quota (see below), so a "100 GB home" is a host filesystem matter, not something this setting enforces.

## Browsers and the CONNECT proxy

A browser in the sandbox cannot call the JSON egress broker, so the runtime also serves an HTTPS `CONNECT` proxy (`ZYVOR_AGENT_PROXY_LISTEN`, default `0.0.0.0:18083`, `off` disables it). Each session's guest gets `ZYVOR_EGRESS_PROXY=http://<session>:<capability>@<gateway>:<port>`. A tunnel goes through the allowlist, `ask`/`sentinel` review, DNS pinning and the private-network gate, and is journaled as `egress.connect` with byte counts. It sees only `host:port`, so credential injection and path review do not apply; only `CONNECT` to `ZYVOR_AGENT_PROXY_CONNECT_PORTS` (default `443`) is served. It constrains only a guest with no other route to the internet. `templates/browser-agent/` has a Chromium template recipe and a Playwright example.

Volumes are per FluxVM tenant and live under FluxVM's `sandbox.volumes_dir` (default `<state_dir>/volumes`). They have no size quota: the limit is the host filesystem. Deleting the agent or a session does not delete the volume; remove the directory on the FluxVM host to discard the data.

## Skills

A skill is a small bundle of instructions and helper files, with a top-level `SKILL.md`, that an agent can read at run time. Publish one with `POST /v1/skills` (`{"name", "description"?, "scope"?, "files": [{"path", "content_base64", "executable"?}]}`) or `fabricctl skill publish <dir>`. Limits: 32 files, 512 KiB per file, 2 MiB in total, relative paths of `[A-Za-z0-9._/-]` only. A skill version is the SHA-256 of its content, so publishing identical content again changes nothing and `GET /v1/skills/{name}` lists every version.

An agent lists skills in its manifest as `name` or `name@version`. Deploy rewrites each to an exact `name@version` pin, so republishing a skill never changes what an already deployed agent version mounts. Every session writes the pinned skills into its sandbox:

| Skill | Mounted at |
|-------|------------|
| No `scope` (base) | `/opt/zyvor/skills/<name>/`, with `/opt/zyvor/skills/INDEX.json` |
| With a `scope` | `/opt/zyvor/skills-scoped/<name>/`, with `INDEX.json` beside it |

Files are mode 0444 (0555 when marked executable). This stops accidental edits by the agent process; it is not a security boundary against a guest that runs as root.

A scoped skill is only usable by an agent whose manifest sets `skill_scope` and whose scope the operator's policy allows. The policy is a JSON file named by `ZYVOR_AGENT_SKILL_SCOPES_FILE`:

```json
{"scopes": {"prod": ["prod"], "internal-test": ["prod", "internal-test"]}}
```

Each key is an agent `skill_scope`; its value lists the skill scopes that agent may mount. With no file, scoped skills cannot be used at all. The policy is checked at deploy and again when each session is provisioned, so tightening it stops new sessions of already deployed agents from mounting a skill they may no longer use. `DELETE /v1/skills/{name}` returns 409 while a deployed agent lists the skill.

## Credentials

Create a **descriptor file**, not a secret file:

```json
{
  "anthropic": {
    "host": "api.anthropic.com",
    "header": "x-api-key",
    "env": "ANTHROPIC_API_KEY",
    "allowed_methods": ["POST"],
    "path_prefixes": ["/v1/"]
  },
  "openai": {
    "host": "api.openai.com",
    "header": "authorization",
    "env": "OPENAI_API_KEY",
    "prefix": "Bearer ",
    "allowed_methods": ["POST"],
    "path_prefixes": ["/v1/"]
  }
}
```

Then put secret values only in the Agent Runtime process environment:

```bash
export ANTHROPIC_API_KEY='...'
export OPENAI_API_KEY='...'
export ZYVOR_AGENT_CREDENTIALS_FILE=/etc/zyvor/agent-credentials.json
```

The descriptor is safe to persist because it contains only the environment variable **name**, never the value.

## Run

```bash
cd agent-runtime
cargo run
```

Configuration:

| Variable | Default | Purpose |
|---|---|---|
| `ZYVOR_AGENT_LISTEN` | `127.0.0.1:9096` | public Agent Runtime API |
| `ZYVOR_AGENT_EGRESS_LISTEN` | `0.0.0.0:18082` | guest-accessible host egress broker |
| `ZYVOR_AGENT_STATE_DIR` | `/var/lib/zyvor-fabric-agent` | deployment/session/event state |
| `ZYVOR_AGENT_SNAPSHOT_DIR` | `/var/lib/fluxvm/agent-snapshots` | FluxVM hibernation snapshots |
| `ZYVOR_AGENT_FLUXVM_URL` | `http://127.0.0.1:7788` | FluxVM API |
| `ZYVOR_AGENT_FLUXVM_TOKEN` | unset | FluxVM bearer token |
| `ZYVOR_AGENT_API_TOKEN` | unset | public Agent Runtime bearer token |
| `ZYVOR_AGENT_KEEP_MODE` | unset | `1` = Keep 0.1 fail-closed: trusted signers required; unsigned policy refused |
| `ZYVOR_AGENT_POLICY_TRUSTED_SIGNERS` | unset | comma-separated hex Ed25519 public keys for `keep.policy.yaml` |
| `ZYVOR_AGENT_POLICY_REQUIRE_SIGNATURE` | auto | ignored when Keep mode is on; otherwise `1`/`0` |
| `ZYVOR_AGENT_SECURITY_PROFILE` | unset / `measured` in Keep mode | passed to FluxVM on sandbox create |
| `ZYVOR_AGENT_CONFINE` | unset | `1` forces `confinement: strict` for every agent |
| `ZYVOR_AGENT_SKILL_SCOPES_FILE` | unset | JSON policy: which skill scopes each agent `skill_scope` may mount (see Skills) |
| `ZYVOR_AGENT_ALLOW_NO_AUTH` | unset | explicit opt-out to start without `ZYVOR_AGENT_API_TOKEN` |
| `ZYVOR_AGENT_CREDENTIALS_FILE` | unset | descriptor JSON above |
| `ZYVOR_AGENT_EGRESS_ADVERTISE_HOST` | derived | host address visible from sandbox |
| `ZYVOR_AGENT_SYNC_INTERVAL_MS` | `300` | guest event sync interval |
| `ZYVOR_AGENT_IDLE_SCAN_INTERVAL_MS` | `1000` | scan interval for safe waiting-session auto-hibernate |
| `ZYVOR_AGENT_WARM_POOL_RECONCILE_INTERVAL_MS` | `2000` | warm-pool health/replenishment interval |
| `ZYVOR_AGENT_WARM_POOL_MAX_CREATE_PER_TICK` | `2` | cap new standby VMs per agent per reconcile pass |
| `ZYVOR_AGENT_WARM_POOL_CLAIM_STALE_SECS` | `300` | clean abandoned durable pool claims after crashes |
| `ZYVOR_AGENT_EXPIRY_SCAN_INTERVAL_MS` | `1000` | runtime-owned session TTL scan interval |

The Agent Runtime refuses to start unless `ZYVOR_AGENT_API_TOKEN` is set, or `ZYVOR_AGENT_ALLOW_NO_AUTH=1` is set to explicitly opt out (only appropriate when something else already restricts access to the API, e.g. a local-only dev loopback bind). For production, also bind the public API behind TLS, firewall port 18082 so only sandbox networks can reach it, and use FluxVM's dataplane/network policy to restrict direct guest egress.

## Write an agent

```ts
import { defineAgent } from "@zyvor/fabric-agent";

export default defineAgent({
  async run(ctx) {
    const response = await ctx.fetch("https://api.anthropic.com/v1/messages", {
      method: "POST",
      credential: "anthropic",
      headers: {
        "content-type": "application/json",
        "anthropic-version": "2023-06-01"
      },
      body: JSON.stringify({
        model: "claude-sonnet-4-5",
        max_tokens: 1024,
        messages: [{ role: "user", content: ctx.input.prompt }]
      })
    });

    return response.json();
  }
});
```

No provider API key appears in `agent.ts` or inside the VM.

## Deploy

```bash
cd sdk/agent-runtime
npm install

FABRIC_AGENT_URL=http://127.0.0.1:9096 \
./src/cli.js deploy ../../examples/agent-runtime/agent.ts \
  --name research-agent \
  --template node22-agent \
  --credential anthropic \
  --allow-host api.anthropic.com \
  --max-concurrency 16 \
  --idle-hibernate 60 \
  --warm-pool 4
```

The deploy command uses esbuild to produce one Node 20 ESM bundle. The deployment version hashes both the executable bundle and its security manifest, so changing an egress/credential grant always creates a new immutable version.

Prefer deploying from the web console instead of the CLI? Build the bundle locally with `fabric-agent build agent.ts --out agent.bundle.mjs` (same esbuild step as `deploy`, minus the POST), then upload it via **Agents → Deploy agent** in the Fabric console. See [Tutorial 13](../docs/tutorials/13-deploy-agent-from-console.md) for the full walkthrough.

`fabric-agent deploy` also sets the newer manifest fields without hand-written JSON: `--egress-mode deny|ask|sentinel`, `--egress-approval-timeout <sec>`, `--home-volume`, `--home-volume-name`, `--home-path`, `--per-user-home`, `--vcpus` with `--memory-mib`, `--skill <name[@version]>` and `--skill-scope`. Flags you leave out are omitted from the manifest, so existing version ids do not change. The SDK's `run()` and `sessions.create()` take `user_id`.

## Start, stream and steer

```ts
import { Fabric } from "@zyvor/fabric-agent";

const fabric = new Fabric({
  baseUrl: process.env.FABRIC_AGENT_URL,
  token: process.env.FABRIC_AGENT_TOKEN
});

const session = await fabric.agent("research-agent").run({
  prompt: "Compare KVM and Firecracker for untrusted agents"
});

for await (const event of session.events()) {
  console.log(event.seq, event.kind, event.data);
}
```

Steer during execution:

```ts
await session.steer({ instruction: "Also cover Cloud Hypervisor" });
```

Agents that want interactive steering can call:

```ts
const message = await ctx.nextSteer({ timeoutMs: 30_000 });
```

## Fast starts with single-use warm pools

Set `warm_pool_size` on an agent deployment (or `--warm-pool <n>` in the CLI). Fabric continuously keeps that many clean FluxVM sandboxes booted, loads only the generic `worker.mjs`, and pauses them.

When a session arrives, Fabric atomically claims the oldest compatible standby, resumes it, writes the immutable agent bundle, starts the worker, and permanently transfers ownership to that session. The VM is **never recycled** after agent code runs. This preserves the same clean-VM isolation model as a cold start.

```ts
const pool = await fabric.agents.warmPool("research-agent");
console.log(pool.desired, pool.ready);

await fabric.agents.reconcileWarmPool("research-agent");

const session = await fabric.agent("research-agent").run(
  { prompt: "Compare KVM and Firecracker" },
  { ttl_seconds: 900, start_policy: "prefer-warm" }
);
console.log(session.start_mode, session.startup_ms); // warm / measured milliseconds
```

Warm-pool records are durable. Health checks first move a standby into an unclaimable `reconciling` lease, so reconciliation can never pause or delete a VM after a concurrent session has claimed it. After a runtime crash, reconciliation removes claims already owned by persisted sessions, deletes abandoned stale claims, drains old deployment versions, repairs accidentally running standby VMs back to paused, discards failed/missing VMs, and replenishes toward the configured target. Worker binaries are SHA-256 pinned so a runtime upgrade drains standbys carrying an older worker.

Session admission supports `start_policy: "prefer-warm" | "require-warm" | "cold-only"`. `prefer-warm` is the default and falls back to a cold FluxVM create; `require-warm` returns a load/backpressure error instead of accepting a slow cold start; `cold-only` bypasses the pool for debugging and latency comparisons.

Session TTL is enforced by the Agent Runtime from the session admission timestamp, not the standby VM creation timestamp. This is required for correct TTL behavior with prewarmed VMs. Expired sessions emit `session.expired`, destroy their FluxVM sandbox, and become terminal. Completed, failed, cancelled, and explicitly deleted sessions also release their single-use sandbox; `sandbox_released` is persisted and a cleanup loop retries idempotent deletion after transient FluxVM failures.

## Reliable retries and fan-out

Pass a stable caller id to make session creation idempotent. A retry with the same agent and `request_id` returns the already-reserved session instead of launching another sandbox:

```ts
const session = await fabric.agent("research-agent").run(
  { prompt: "Investigate KVM" },
  { request_id: "ticket:INC-1042" }
);
```

For bounded fan-out across many independent sessions:

```ts
const sessions = await fabric.sessions.createMany(
  targets.map((target) => ({
    agent: "research-agent",
    input: { target },
    request_id: `scan:${target}`
  })),
  { concurrency: 8 }
);
```

`max_concurrent_sessions` is enforced server-side against all non-terminal sessions for an agent, including hibernated sessions that still reserve a sandbox. This prevents retry storms and unbounded per-agent VM creation.

## Safe automatic hibernation

`idle_hibernate_seconds` does **not** treat a quiet log stream as proof of idleness. The guest worker exposes `waiting` only while user code is blocked inside `ctx.nextSteer()`. The host auto-hibernator checks both the configured idle duration and that explicit guest state before snapshot+pause. CPU-bound or tool-running code is never auto-frozen simply because it emitted no events.

## Credential request scopes

Credential descriptors can reduce a provider key to specific methods, URL path prefixes, and non-standard TLS ports:

```json
{
  "github-writer": {
    "host": "api.github.com",
    "header": "authorization",
    "env": "GITHUB_TOKEN",
    "prefix": "Bearer ",
    "allowed_methods": ["GET", "POST"],
    "path_prefixes": ["/repos/zyvorai/"],
    "allowed_ports": []
  }
}
```

Port 443 is always allowed for HTTPS. Any other credential-bearing port must be explicitly listed. Empty `allowed_methods` or `path_prefixes` keeps that dimension unrestricted.

## HTTP API

```text
POST   /v1/agents
GET    /v1/agents
GET    /v1/agents/{name}
GET    /v1/agents/{name}/warm-pool
POST   /v1/agents/{name}/warm-pool   # trigger one reconcile pass

POST   /v1/sessions
GET    /v1/sessions
GET    /v1/sessions/{id}
DELETE /v1/sessions/{id}
POST   /v1/sessions/{id}/steer
POST   /v1/sessions/{id}/cancel
POST   /v1/sessions/{id}/hibernate
POST   /v1/sessions/{id}/resume
GET    /v1/sessions/{id}/events?after=<seq>
POST   /v1/sessions/{id}/delegate

POST   /v1/schedules
DELETE /v1/schedules/{id}
POST   /v1/webhooks
DELETE /v1/webhooks/{id}
POST   /v1/hooks/{id}                 # HMAC signature, not the API bearer token
POST   /v1/loops
DELETE /v1/loops/{id}
GET    /v1/approvals
POST   /v1/approvals
POST   /v1/approvals/{id}
GET    /v1/audit?session_id=&limit=
GET    /v1/skills
POST   /v1/skills
GET    /v1/skills/{name}
DELETE /v1/skills/{name}

POST   /mcp
```

The events endpoint is SSE. Reconnecting with the last seen sequence number replays all durable host events after that point.

## Hibernation semantics

`hibernate` asks the guest worker to checkpoint its event boundary, creates a FluxVM memory+disk sandbox snapshot, and pauses the sandbox. `resume` resumes that same paused sandbox, preserving the JavaScript process stack and in-memory agent state.

Session metadata, TTL deadlines, warm-pool claims and the host event journal survive Agent Runtime restarts. Exact mid-stack recovery after a **host/FluxVM process restart** additionally requires FluxVM to expose its existing sandbox `SnapshotLoad` primitive through REST. The current FluxVM API can save a snapshot but cannot load one, so cold restore is not implemented.

## Continuous integration

`.github/workflows/agent-runtime.yml` typechecks the crate, builds the example bundles, and runs a real session on the GitHub runner. Runners have no FluxVM, so [`agent-runtime/tests/sandbox_stub.py`](tests/sandbox_stub.py) stores the guest files and starts `worker.mjs` or `harness.mjs` with Node on the runner. Provider APIs are not called.

**Keep** (policy YAML, cockpit routes, merionctl, docs) has its own workflow:
[`.github/workflows/keep.yml`](../.github/workflows/keep.yml). Locally:

```bash
cargo test --manifest-path agent-runtime/Cargo.toml --lib
cargo test --manifest-path agent-runtime/Cargo.toml policy -- --nocapture
./scripts/merion-e2e.sh
./scripts/merion-live-lab.sh   # FluxVM host; guest boot needs KEEP_E2E_TEMPLATE
./scripts/merionctl --help
```

Tutorial: [docs/tutorials/16-merion-workstation.md](../docs/tutorials/16-merion-workstation.md).  
Production gate: [docs/merion/PRODUCTION.md](../docs/merion/PRODUCTION.md).

[`agent-runtime/tests/session-ci.sh`](tests/session-ci.sh) deploys four agents and checks each path:

| Agent | What the job asserts |
|---|---|
| `examples/agent-runtime/ops-agent.ts` | Session reaches `completed` and the journal contains `healthcheck.completed` |
| `agent-runtime/tests/waiter.mjs` | Stays in `nextSteer()` so another agent can be delegated before the session ends |
| `agent-runtime/tests/harness-prompt.md` | A fake `claude` on `PATH` prints `ZYVOR_APPROVAL`; approving it finishes the session |
| `examples/agent-runtime/hello-go-agent.ts` | Returns the Go source it wrote, and `go run` prints `hello` |

The same script also checks MCP `tools/list` and `chat_with_agent`, a five-field cron schedule, a signed webhook (and a rejected bad signature), and a loop with `max_runs: 1`.

```bash
bash agent-runtime/tests/session-ci.sh
```

The harness writes its workspace to `/opt/zyvor/agent/workspace`. GitHub-hosted runners create that directory. If the path cannot be created, the script sets `ZYVOR_HARNESS_WORKSPACE` and the harness uses that directory instead.

A push to `main` also runs [`.github/workflows/lab-deploy.yml`](../.github/workflows/lab-deploy.yml), which rebuilds `zyvor-fabricd` on the lab host and runs the end-to-end checks and the API audit. That job is separate from the agent session job above.

## Security notes

- provider secrets are loaded only in the host egress broker
- session capabilities are random and excluded from `SessionView`
- credential use requires an explicit agent grant
- credential descriptor host must match the request host
- agent egress host allowlist must match the request host
- credential injection is HTTPS-only
- brokered private, loopback and link-local destinations are blocked by default (explicit manifest opt-in only)
- guest-supplied `Authorization`, proxy auth, hop-by-hop headers, and any configured credential-injection header are stripped before forwarding
- upstream redirects are disabled, preventing credential forwarding to a redirected host
- agent versions include the security manifest in their digest
- guest HTTP error bodies are fixed strings (`upstream request failed`, `request failed`). The exception is written to the guest process log, not the response. A failed session records `error.message` in `session.failed`, not the stack.

This broker protects managed provider credentials. For a full no-bypass sandbox, pair it with FluxVM Network Fabric default-deny rules so the guest can reach only the broker/DNS and required internal services.
