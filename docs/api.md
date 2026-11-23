# Zyvor Fabric REST API Documentation

## Base URL

```
http://localhost:9095/api
```

## Authentication

Most endpoints require a valid JWT token passed via the `Authorization` header:

```
Authorization: Bearer <token>
```

**Obtain a token:**

```bash
# Read the auto-generated admin password (first startup)
PASSWORD=$(sudo cat /var/lib/zyvor-fabricd/.admin_password)

# Login
curl -X POST http://localhost:9095/api/auth/login \
  -H "Content-Type: application/json" \
  -d "{\"username\": \"admin\", \"password\": \"$PASSWORD\"}"
```

Unauthenticated requests receive a `401 Unauthorized` response. The `/api/auth/login`, `/health`, and `/readyz` endpoints are accessible without authentication. When auth is disabled in config, all endpoints are accessible without a token.

`GET /readyz` returns JSON `{"ok", "store", "fluxvm"}` (HTTP 503 when not ready). Use it for load-balancer / Kubernetes readiness; keep `/health` for liveness.

## Overview

The API exposes 780+ REST endpoints and 3 WebSocket endpoints organized into the categories below. This document lists the key endpoints in each category. All request and response bodies use JSON.

---

## Auth

User authentication and session management.

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/auth/sign-in` | Authenticate and obtain a token |
| POST | `/auth/logout` | Invalidate the current token |
| POST | `/auth/refresh` | Refresh an expiring token |
| GET | `/auth/me` | Get the current authenticated user |
| POST | `/auth/users` | Create a new user |
| GET | `/auth/users` | List users |
| PUT | `/auth/users/:id` | Update a user |
| DELETE | `/auth/users/:id` | Delete a user |
| POST | `/auth/roles` | Create a role |
| GET | `/auth/roles` | List roles |

## Enterprise Identity (SCIM)

SCIM 2.0 lifecycle provisioning and group-to-role sync for Entra ID / Okta,
layered on top of an existing OIDC/SAML/LDAP auth provider. Admin routes use
a normal Fabric JWT; the `/scim/v2/*` data-plane routes use a dedicated,
profile-scoped bearer token instead (minted below), not a Fabric JWT. See
[docs/scim-identity.md](scim-identity.md) for the full walkthrough and
security properties.

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/identity/scim/profiles` | List provisioning profiles |
| POST | `/identity/scim/profiles` | Create a provisioning profile |
| PUT | `/identity/scim/profiles/:id` | Update a profile |
| DELETE | `/identity/scim/profiles/:id` | Delete a profile (must have no active SCIM resources) |
| GET | `/identity/scim/tokens` | List SCIM bearer tokens |
| POST | `/identity/scim/tokens` | Mint a SCIM bearer token for a profile (plaintext shown once) |
| DELETE | `/identity/scim/tokens/:id` | Revoke a SCIM bearer token |

SCIM data-plane routes (outside `/api`, under `/scim/v2`, bearer-token auth):

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/scim/v2/ServiceProviderConfig` | SCIM discovery: supported features |
| GET | `/scim/v2/ResourceTypes` | SCIM discovery: resource types |
| GET | `/scim/v2/Schemas` | SCIM discovery: schemas |
| GET | `/scim/v2/Users` | List/filter users |
| POST | `/scim/v2/Users` | Create a user |
| GET | `/scim/v2/Users/:id` | Get a user |
| PUT | `/scim/v2/Users/:id` | Replace a user |
| PATCH | `/scim/v2/Users/:id` | Patch a user (activate/deactivate, etc.) |
| DELETE | `/scim/v2/Users/:id` | Deprovision a user |
| GET | `/scim/v2/Groups` | List/filter groups |
| POST | `/scim/v2/Groups` | Create a group |
| GET | `/scim/v2/Groups/:id` | Get a group |
| PUT | `/scim/v2/Groups/:id` | Replace a group |
| PATCH | `/scim/v2/Groups/:id` | Patch a group (membership changes) |
| DELETE | `/scim/v2/Groups/:id` | Delete a group |

## OpenStack Compatibility

Experimental OpenStack wire-protocol façade (Keystone v3, Nova v2.1, Glance v2,
Neutron v2.0, Cinder v3) mounted on the **same listen port** as Fabric, outside
`/api`. Not the same as SCIM (`/scim/v2`). Catalog endpoint URLs come from
`daemon.public_url` / `ZYVOR_FABRICD_PUBLIC_URL` (else listen + TLS). See [docs/openstack-compat.md](openstack-compat.md) and the hands-on
[Tutorial 08](tutorials/08-openstack-clients.md).

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/identity/v3/auth/tokens` | Issue Keystone-style token (`X-Subject-Token`) |
| GET | `/identity/v3/auth/tokens` | Validate token |
| GET | `/identity/v3/auth/catalog` | Service catalog (requires token) |
| GET | `/compute/v2.1/flavors` | List Nova flavors (`m1.tiny` … `m1.xlarge`) |
| GET/POST | `/compute/v2.1/servers` | List / create servers |
| POST | `/compute/v2.1/servers/:id/action` | start / stop / reboot |
| GET | `/image/v2/images` | List Glance images |
| GET | `/network/v2.0/networks` | List Neutron networks |
| GET/POST | `/volume/v3/:project_id/volumes` | List / create Cinder volumes |

## VM Management

Core virtual machine lifecycle operations.

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/app/vms` | List all VMs |
| POST | `/app/vms` | Create a new VM |
| GET | `/app/vms/:name` | Get VM details |
| DELETE | `/app/vms/:name` | Delete a VM |
| POST | `/app/vms/:name/start` | Start a VM |
| POST | `/app/vms/autostart/reconcile?dry_run=` | Start VMs tagged `autostart` that are not running (ordered, pre-flight checked). `dry_run=true` answers 200 with the report; a live run answers 202 with a job `id` (409 while another live run is active) |
| GET | `/app/vms/autostart/reconcile` | List recent autostart runs (newest first, last 20 kept in memory; the startup pass is included) |
| GET | `/app/vms/autostart/reconcile/:id` | Progress of one run: per-VM status as it happens, then the final report |
| GET/PUT/DELETE | `/app/vms/{name}/snapshot-policy` | Per-VM snapshot schedule (interval or cron) and retention (keep_last, max_age_secs) |
| PUT | `/app/vms/{name}/snapshots/{id}/tags` | Replace a snapshot's tags (`{"tags": [...]}`); the `keep` tag protects it from policy pruning |
| POST | `/app/vms/{name}/snapshot-policy/run?dry_run=` | Take a policy snapshot now and prune (admin), or preview with `dry_run=true` |
| POST | `/app/vms/:name/preflight` | Read-only pre-flight check: can this VM start here? (`can_start` plus findings) |
| POST | `/app/vms/:name/stop` | Stop a VM |
| POST | `/app/vms/:name/restart` | Restart a VM |
| POST | `/app/vms/:name/pause` | Pause a VM |
| POST | `/app/vms/:name/resume` | Resume a paused VM |
| GET | `/app/vms/:name/metrics` | Get VM resource metrics |
| GET | `/app/vms/:name/status` | Get detailed VM status |

### Example: Create a VM

```
POST /api/vms
Content-Type: application/json

{
  "name": "myvm",
  "image": "/path/to/image.qcow2",
  "cpus": 4,
  "memory": 4096
}
```

**Response (201 Created):**
```json
{
  "name": "myvm",
  "state": "stopped",
  "cpus": 4,
  "memory": 4096,
  "image": "/path/to/image.qcow2"
}
```

### Start a VM with Options

The `POST /vms/:name/start` endpoint accepts an optional JSON body with `VMStartOptions` to control how the active VM driver launches the VM. All fields are optional and default sensibly when omitted.

`VMStartOptions` only applies to a VM's first launch (translated into an FluxVM `CreateVmRequest`). Most fields are honored: cpus/memory (from the VM record), `network_tap`/`network_user_mode`, `linux`/`initrd`/`firmware`, `extra_args`, and `bind_mounts` (each entry becomes a launch-time virtiofs share, auto-mounted in the guest via cloud-init — no live bind-mount equivalent exists for a real hardware VM). The rest have no FluxVM equivalent and fail with a clear error rather than silently being ignored: `tpm`, `secure_boot`, `vsock`, `extra_drives`, `bind_users`, `credentials`/`load_credentials`, `smbios11`, `directory`.

```
POST /api/vms/myvm/start
Content-Type: application/json

{
  "scope": "system",
  "directory": "/path/to/rootfs",
  "kvm": true,
  "secure_boot": false,
  "vsock": true,
  "vsock_cid": 42,
  "tpm": true,
  "tpm_state": "auto",
  "linux": "/boot/vmlinuz",
  "initrd": ["/boot/initrd.img"],
  "network_tap": true,
  "network_user_mode": false,
  "firmware": "/usr/share/ovmf/OVMF.fd",
  "discard_disk": true,
  "grow_image": "50G",
  "smbios11": ["custom.vendor-string=hello"],
  "notify_ready": true,
  "uuid": "550e8400-e29b-41d4-a716-446655440000",
  "slice": "vm.slice",
  "properties": ["MemoryMax=4G", "CPUQuota=200%"],
  "register": true,
  "private_users": "1000:65536",
  "bind_mounts": [
    { "source": "/host/data", "destination": "/vm/data", "read_only": false }
  ],
  "extra_drives": ["/extra/disk.raw"],
  "bind_users": ["myuser"],
  "bind_user_shell": "/bin/bash",
  "bind_user_groups": ["wheel"],
  "forward_journal": "/var/log/vm.journal",
  "pass_ssh_key": true,
  "ssh_key_type": "ed25519",
  "console": "interactive",
  "background": "44",
  "quiet": false,
  "credentials": [
    { "id": "passwd.hashed-password.root", "value": "$y$..." }
  ],
  "load_credentials": [
    { "id": "ssh.authorized_keys.root", "path": "/root/.ssh/authorized_keys" }
  ],
  "extra_args": ["enforcing=0"]
}
```

#### VMStartOptions Field Reference

| Field | Type | Description |
|-------|------|-------------|
| `scope` | `"system"` \| `"user"` | Manager scope (omitted = auto: system when root, user otherwise) |
| `directory` | string | Root filesystem directory (alternative to image) |
| `kvm` | bool? | KVM acceleration (`null` = omitted, uses Zyvor Fabric default) |
| `secure_boot` | bool? | Secure Boot firmware (`null` = omitted, uses Zyvor Fabric default) |
| `vsock` | bool? | VSock networking (`null` = omitted, uses Zyvor Fabric default) |
| `vsock_cid` | u32? | VSock CID (`null` = omitted, uses Zyvor Fabric default) |
| `tpm` | bool? | TPM support (`null` = omitted, uses Zyvor Fabric default) |
| `tpm_state` | string | TPM state path, `"auto"`, or `"off"` |
| `linux` | string | Kernel image path for direct kernel boot |
| `initrd` | string[] | Initrd paths (multiple are merged) |
| `network_tap` | bool | Create a TAP device, requires root (default: `false`) |
| `network_user_mode` | bool | Use user mode networking (default: `false`) |
| `firmware` | string | Firmware definition file path |
| `discard_disk` | bool? | Process discard/trim requests (`null` = omitted, Zyvor Fabric default: yes) |
| `grow_image` | string | Grow image to size, e.g. `"50G"`. Validated format: digits + optional size suffix |
| `smbios11` | string[] | SMBIOS Type #11 vendor strings (must not start with `io.systemd.credential`) |
| `notify_ready` | bool? | Wait for READY=1 from VM init (`null` = omitted, Zyvor Fabric default: yes) |
| `uuid` | string | Machine UUID (must be valid UUID format: `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`) |
| `slice` | string | Systemd slice for scope unit (must end with `.slice`) |
| `properties` | string[] | Resource-control properties only (e.g. `"MemoryMax=4G"`, `"CPUQuota=200%"`) |
| `register` | bool? | Unused — no equivalent on the current (FluxVM) driver |
| `private_users` | string | User namespace mapping (e.g. `"1000:65536"`) |
| `bind_mounts` | BindMount[] | Host-to-VM bind mounts (paths must not contain `..`) |
| `extra_drives` | string[] | Additional disk images or block devices |
| `bind_users` | string[] | Host users to bind into the VM (system users and UIDs < 1000 are blocked) |
| `bind_user_shell` | string | Shell for bound users: `"yes"`, `"no"`, or an absolute path |
| `bind_user_groups` | string[] | Auxiliary groups for bound users |
| `forward_journal` | string | Forward VM journal to host (file or dir) |
| `pass_ssh_key` | bool? | Generate and pass SSH key (`null` = omitted, Zyvor Fabric default: yes) |
| `ssh_key_type` | enum | SSH key type: `"ed25519"`, `"ecdsa"`, `"rsa"` |
| `console` | enum | Console mode: `"interactive"`, `"read-only"`, `"native"`, `"gui"` |
| `background` | string | Terminal background color (ANSI SGR) |
| `quiet` | bool | Suppress Zyvor Fabric status output (default: `false`) |
| `credentials` | Credential[] | Credentials via `--set-credential` |
| `load_credentials` | LoadCredential[] | Credentials loaded from file via `--load-credential` |
| `extra_args` | string[] | Extra kernel command line arguments (must not start with `-`) |

**BindMount object:** `{ "source": string, "destination": string?, "read_only": bool }`

**Credential object:** `{ "id": string, "value": string }` — `id` must be alphanumeric/dot/hyphen/underscore (no colons)

**LoadCredential object:** `{ "id": string, "path": string }` — `path` is the file to load the credential from (`"value"` is accepted as an alias for backward compatibility)

See the note above for which fields FluxVM actually honors versus rejects with an error.

## Snapshots

Point-in-time VM state capture and restoration.

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/app/vms/:name/snapshots` | List snapshots for a VM |
| POST | `/app/vms/:name/snapshots` | Create a snapshot |
| GET | `/app/vms/:name/snapshots/:id` | Get snapshot details |
| DELETE | `/app/vms/:name/snapshots/:id` | Delete a snapshot |
| POST | `/app/vms/:name/snapshots/:id/revert` | Revert VM to snapshot |

## Storage

Disk and volume management.

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/app/storage/pools` | List storage pools |
| POST | `/app/storage/pools` | Create a storage pool |
| GET | `/app/storage/pools/:id` | Get pool details |
| DELETE | `/app/storage/pools/:id` | Delete a storage pool |
| GET | `/app/storage/volumes` | List volumes |
| POST | `/app/storage/volumes` | Create a volume |
| DELETE | `/app/storage/volumes/:id` | Delete a volume |
| POST | `/app/storage/volumes/:id/resize` | Resize a volume |
| POST | `/app/storage/volumes/:id/attach` | Attach volume to a VM |
| POST | `/app/storage/volumes/:id/detach` | Detach volume from a VM |

## Distributed Storage

Cluster-wide storage management.

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/distributed-storage/clusters` | List storage clusters |
| POST | `/distributed-storage/clusters` | Create a storage cluster |
| GET | `/distributed-storage/clusters/:id` | Get cluster details |
| DELETE | `/distributed-storage/clusters/:id` | Delete a storage cluster |

## Networking

Virtual network and interface management.

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/networks` | List virtual networks |
| POST | `/networks` | Create a virtual network |
| GET | `/networks/:id` | Get network details |
| PUT | `/networks/:id` | Update a network |
| DELETE | `/networks/:id` | Delete a network |
| GET | `/app/vms/:name/interfaces` | List VM network interfaces |
| POST | `/app/vms/:name/interfaces` | Attach a network interface |
| DELETE | `/app/vms/:name/interfaces/:id` | Detach a network interface |

## System

Daemon health, configuration, and system information.

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/health` | Liveness check (plain OK) |
| GET | `/readyz` | Readiness (store + FluxVM `/readyz`; 503 if not ready) |
| GET | `/app/system/info` | System information |
| GET | `/app/system/config` | Get daemon configuration |
| PUT | `/app/system/config` | Update daemon configuration |
| GET | `/metrics` | Prometheus-format metrics |
| GET | `/system/capacity` | Host CPU/memory capacity with the overcommit policy applied, allocated and available |
| GET | `/system/overcommit` | Overcommit policy (admin `PUT` to change) |
| POST | `/capacity/whatif` | Capacity what-if (read-only): which of these new VMs would fit, and why not |
| POST | `/hosts/{id}/drain-plan` | Host drain plan (read-only dry run): where each VM would migrate, stop or be blocked, with a plan hash |
| GET | `/snapshots/health` | Snapshot health scoreboard (read-only): RPO status, broken chains, space forecast per VM |
| GET | `/cpu/inventory` | CPU inventory (read-only): host CPU records, VM CPU models, per-cluster baseline model |
| GET, PUT | `/hosts/{id}/cpu-inventory` | Read or record a host's CPU vendor, model and flags (`PUT` needs write) |
| GET | `/vms/{name}/cpu-compat?host=` | Can this VM run on a host? Stored CPU model against host CPU flags (read-only) |
| GET | `/vms/{name}/history` | VM configuration revisions (redacted, hash-chained to the audit log), newest first |
| GET | `/vms/{name}/history/{rev}` | One revision with its redacted configuration |
| GET | `/vms/{name}/history/diff?from=&to=` | Field-level diff between two revisions with risk flags |
| GET | `/vms/{name}/history/since-good` | Diff from the last known good revision to now, likely culprits ranked, with a summary |
| POST | `/vms/{name}/history/{rev}/mark-good` | Mark a revision as known good (write access, audited) |
| POST | `/vms/bulk` | Bulk VM operation by selector (start, stop, snapshot, tag-add, tag-remove): dry-run plan by default, apply with the plan hash |

### Host drain plan

`POST /api/hosts/{id}/drain-plan` (id or hostname; read access; no body; 404 for an unknown host; 403 for tenant-scoped tokens) never moves or stops anything. A VM belongs to a host when its label `fabric.zyvor.dev/host` equals the host id or hostname; VMs without it are not planned and the response warns with their count. Targets are other `connected` hosts in the same cluster. Capacity is `floor(total * ratio)` (overcommit policy when enabled) against the vCPUs and memory of running VMs on the host. DRS affinity rules are applied: `required` rules block a target, `preferred` rules only steer. Label `fabric.zyvor.dev/eviction` per VM: `migrate` (default), `migrate-if-possible` (becomes `stop` when it cannot move), `stop`, `pin` (blocks the drain).

```json
{
  "host_id": "h1", "dry_run": true, "can_drain": false,
  "actions": [
    { "order": 1, "vm": "web", "action": "migrate", "target": "h2", "reasons": [], "warnings": [] },
    { "order": 2, "vm": "db", "action": "blocked", "target": null, "reasons": ["memory"], "warnings": [] }
  ],
  "summary": { "migrate": 1, "relocate": 0, "stop": 0, "blocked": 1 },
  "targets": [{ "host_id": "h2", "vcpus_used_after": 1, "vcpus_capacity": 8, "memory_mb_used_after": 512, "memory_mb_capacity": 4096 }],
  "warnings": [], "plan_hash": "<sha256 hex>"
}
```

Order: migrations (largest memory first), relocations, stops, then blocked VMs. `plan_hash` changes when any action or target load changes. CLI: `fabricctl drain-plan <host>`. Web: Host Drain Plan.

### CPU inventory and compatibility

Fabric stores a VM's CPU document (`GET/POST /api/vms/{name}/cpu-model`: `mode` `custom`, `host-model` or `host-passthrough`, `model`, `features_add`, `features_remove`, `vendor`) but the host agent heartbeat carries no CPU data, so host CPUs are recorded explicitly: `PUT /api/hosts/{id}/cpu-inventory` with `{"vendor":"GenuineIntel","model_name":"...","flags":["avx2","aes"]}` (at most 1024 flags of 48 characters from `a-z 0-9 _ . -`; 400 otherwise) or `{"from_local":true}`, which reads `/proc/cpuinfo` (bounded to 256 KiB) on the daemon host and is only right when the daemon runs on that host. `GET /api/cpu/inventory` lists hosts with their CPU records, VMs with a stored CPU document and, per cluster, the highest named model every host with an inventory can run (`baseline_model`; names come from a built-in table that approximates the QEMU definitions: `qemu64`, `x86-64-v2`/`v3`/`v4`, Nehalem to Icelake-Server, EPYC, EPYC-Rome, EPYC-Milan). Tenant-scoped tokens get 403.

`GET /api/vms/{name}/cpu-compat[?host=id-or-hostname]` returns the VM's requirement and a `verdict` per host: `compatible`, `unknown` (no inventory for that host) or `incompatible` (with `missing` features and `vendor_mismatch`). Requirement rules:

* No stored CPU document: no requirement (the default document is deliberately not enforced).
* `custom`: the model's features plus `features_add` minus `features_remove`, and `vendor` (`intel`, `amd` or a vendor string). An unknown model or a feature outside the compared set is ignored and listed in `notes`, so the check never invents a blocker.
* `host-model` / `host-passthrough`: a running VM needs the target to offer the feature set of the host it runs on (label `fabric.zyvor.dev/host`, which needs that host's inventory) and the same vendor; a stopped VM has no requirement.

Only guest-visible ISA features (the union of the model table) are compared, never mitigation or kernel flags. The result feeds two existing checks: the drain planner rejects a target with the reason `cpu_incompatible` (the blocked action's `warnings` say which features are missing; a target without an inventory is allowed and the action warns `not verified`, and verified targets are preferred), and the start preflight reports the Blocker `cpu.incompatible` for a `custom` VM whose model this host cannot provide (Info `cpu.unverified` when `/proc/cpuinfo` cannot be read). CLI: `fabricctl cpu-inventory`, `cpu-report <host> [--from-local | --vendor V --model M --flags "a b"]`, `cpu-compat <vm> [--host H]`. Web: CPU Compatibility.

Limit: the FluxVM driver does not read the stored CPU document today, so this is a planning and safety check on declared intent, not a guarantee about what the guest sees.

### VM configuration history

`GET /api/vms/{name}/history` lists redacted, bounded, content-hashed revisions of a VM's configuration with actor, cause and a link (`audit.id`, `seq`, `hash`) to the `VM_CONFIG_REVISION` audit event each one wrote; `.../history/{rev}` returns one with its configuration; `.../history/diff?from=&to=` explains each changed field and flags risky ones (CPU, memory or disk shrink, disk removal or bus change, NIC model or MAC, firmware or machine type, boot order); `.../history/since-good` returns the diff from the last known good revision with a one-paragraph summary and ranked likely culprits. A revision is good automatically when the VM stayed Running for `[config_history] good_after_minutes` (lifecycle events, not guest health) or when marked with `POST .../mark-good`. Secrets are never stored in clear. Nothing changes or reverts the VM. Full reference: [config-history.md](config-history.md). CLI: `fabricctl vm history|diff|since-good|mark-good`. Web: VM detail, History tab.

### Bulk VM operations

`POST /api/vms/bulk` (write access; 403 for tenant-scoped tokens) selects VMs and plans one action each. Body:

```json
{
  "selector": { "tags": ["web"], "labels": {"tier": "a"}, "names": [], "name_prefix": "web-" },
  "action": "stop",
  "tag": "only for tag-add / tag-remove",
  "snapshot_prefix": "bulk",
  "max_vms": 10,
  "dry_run": true,
  "plan_hash": "<from the dry run, required to apply>",
  "continue_on_error": false
}
```

All selector criteria must match, an empty selector is 400, and a selector matching more than 500 VMs is 400. The default is a dry run, which returns `plan.items` (`will`: the action or `skip` with a `reason` such as `already_running`, `not_running`, `already_tagged`, `state_transitioning` or `preflight_blocked: <ids>` for `start`), a summary, `over_cap`, `can_apply` and a SHA-256 `plan_hash`. To apply, repeat the request with `dry_run:false` and that hash: the plan is recomputed first and a different hash is `409` with the new plan. `max_vms` (default 10, hard limit 50) caps how many VMs may be acted on; an over-cap plan cannot be applied (400). VMs are processed one at a time in name order through the same guarded code as the single-VM routes (per-VM lock; `start` runs the preflight and waits for the start task, `snapshot` is a Disk snapshot named `<prefix>-<UTC timestamp>`), and the first failure stops the run unless `continue_on_error` is true (the rest are `not_attempted`). One bulk run at a time: a second gets 409. Each VM writes an audit entry `BULK_START`, `BULK_STOP`, `BULK_SNAPSHOT`, `BULK_TAG_ADD` or `BULK_TAG_REMOVE`, and the run writes `BULK_VMS` with the first 16 hex characters of the plan hash. The request is held open while VMs start, so keep the cap small. CLI: `fabricctl vm-bulk --action stop --select-tag web [--apply --plan-hash H]`. Web: Bulk VM Operations.

### Snapshot health

`GET /api/snapshots/health?default_rpo_secs=86400` (read access; tenant-scoped tokens see only their VMs; `default_rpo_secs` at least 60) returns a summary and one row per VM, worst first. Status compares the newest snapshot's age with the RPO target: `healthy` at or under the target, `at_risk` up to twice the target, `breached` beyond, `no_snapshots` when none. The target is the policy `interval_secs`, the gap between two cron runs, or `default_rpo_secs` when there is no enabled policy (`rpo_source` says which). `orphaned` lists snapshot ids whose `parent_id` no longer exists. `growth_bytes_per_day` is the last 7 days of snapshot bytes divided by 7; `projected_bytes_30d` adds 30 days of growth, capped by `keep_last` retention. CLI: `fabricctl snapshot-health [--default-rpo-secs N]`. Web: Snapshot Health.

### Capacity what-if

`POST /api/capacity/whatif` needs read access and never creates, reserves or changes anything. It uses the same inputs as `GET /api/system/capacity`: this host's CPUs and memory, the stored VMs, and the overcommit policy (ratios apply only when the policy is enabled).

```json
{
  "vms": [{ "name": "web", "vcpus": 2, "memory_mb": 4096, "disk_gb": 40, "count": 3 }],
  "disk_gb_total": 500
}
```

`disk_gb_total` is optional; when omitted, disk is not checked (the daemon does not track VM disk capacity). Limits: 1-200 entries, `count` 1-1000, at most 5000 VMs in total. VMs are placed first-fit-decreasing (memory, then vCPUs, then disk, then name), so the result is deterministic. The response lists `placements`, `unplaced` (each with `reasons` of `vcpu`, `memory`, `disk` or `no_single_host`) and per-host `headroom_after`.

## Quotas

Resource usage limits per user or project.

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/app/quotas` | List all quotas |
| POST | `/app/quotas` | Create a quota |
| GET | `/app/quotas/:id` | Get quota details |
| PUT | `/app/quotas/:id` | Update a quota |
| DELETE | `/app/quotas/:id` | Delete a quota |
| GET | `/app/quotas/:id/usage` | Get current usage against quota |

## Schedules

Scheduled VM operations (start, stop, snapshot, backup).

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/app/schedules` | List schedules |
| POST | `/app/schedules` | Create a schedule |
| GET | `/app/schedules/:id` | Get schedule details |
| PUT | `/app/schedules/:id` | Update a schedule |
| DELETE | `/app/schedules/:id` | Delete a schedule |
| POST | `/app/schedules/:id/trigger` | Manually trigger a schedule |

## Audit

Administrative action audit trail.

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/app/audit/logs` | Query audit log entries |
| GET | `/app/audit/logs/:id` | Get a specific audit entry |
| GET | `/app/audit/summary` | Get audit summary statistics |
| GET | `/api/audit/export` | Stream filtered events as NDJSON or CSV (see [audit-export.md](audit-export.md)) |
| GET | `/api/audit/verify` | Check the audit hash chain |
| POST | `/api/audit/retention` | Prune by age/size, with dry run |

## Analytics

Usage and performance analytics.

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/app/analytics/overview` | Platform-wide analytics summary |
| GET | `/app/analytics/vms` | Per-VM analytics |
| GET | `/app/analytics/resources` | Resource utilization trends |
| GET | `/app/analytics/reports` | Generate or list reports |

## Backups

VM backup and restore operations.

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/app/backups` | List backups |
| POST | `/app/backups` | Create a backup |
| GET | `/app/backups/:id` | Get backup details |
| DELETE | `/app/backups/:id` | Delete a backup |
| POST | `/app/backups/:id/restore` | Restore a VM from backup |
| GET | `/app/backups/policies` | List backup policies |
| POST | `/app/backups/policies` | Create a backup policy |

## Notifications

Alert and notification management.

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/notifications` | List notifications |
| POST | `/notifications` | Create a notification rule |
| PUT | `/notifications/:id` | Update a notification rule |
| DELETE | `/notifications/:id` | Delete a notification rule |
| POST | `/notifications/:id/acknowledge` | Acknowledge a notification |
| GET | `/notifications/channels` | List notification channels |
| POST | `/notifications/channels` | Create a notification channel |

## Templates

VM templates for standardized provisioning.

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/app/templates` | List templates |
| POST | `/app/templates` | Create a template |
| GET | `/app/templates/:id` | Get template details |
| PUT | `/app/templates/:id` | Update a template |
| DELETE | `/app/templates/:id` | Delete a template |
| POST | `/app/templates/:id/deploy` | Deploy a VM from template |

## Tags

Resource tagging and categorization.

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/tags` | List all tags |
| POST | `/tags` | Create a tag |
| DELETE | `/tags/:id` | Delete a tag |
| POST | `/app/vms/:name/tags` | Tag a VM |
| DELETE | `/app/vms/:name/tags/:tag` | Remove a tag from a VM |

## Cloning

VM cloning (full and linked).

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/app/vms/:name/clone` | Clone a VM |
| GET | `/app/vms/:name/clones` | List clones of a VM |

## DRS (Distributed Resource Scheduler)

Automatic VM placement and load balancing.

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/app/drs/config` | Get DRS configuration |
| PUT | `/app/drs/config` | Update DRS configuration |
| GET | `/app/drs/recommendations` | Get placement recommendations |
| POST | `/app/drs/recommendations/:id/apply` | Apply a recommendation |

## Fault Tolerance

VM fault tolerance configuration.

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/app/vms/:name/ft` | Get fault tolerance status |
| POST | `/app/vms/:name/ft/enable` | Enable fault tolerance |
| POST | `/app/vms/:name/ft/disable` | Disable fault tolerance |

## Replication

VM replication to secondary hosts.

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/app/replication/configs` | List replication configurations |
| POST | `/app/replication/configs` | Create a replication config |
| GET | `/app/replication/configs/:id` | Get replication config details |
| DELETE | `/app/replication/configs/:id` | Delete a replication config |
| POST | `/app/replication/configs/:id/sync` | Trigger manual sync |

## Site Recovery

Disaster recovery and failover.

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/app/site-recovery/plans` | List recovery plans |
| POST | `/app/site-recovery/plans` | Create a recovery plan |
| POST | `/app/site-recovery/plans/:id/test` | Test a recovery plan |
| POST | `/app/site-recovery/plans/:id/execute` | Execute failover |
| POST | `/app/site-recovery/plans/:id/reprotect` | Reprotect after failover |

## Content Library

Shared image and template repository.

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/app/content-library/items` | List library items |
| POST | `/app/content-library/items` | Upload an item |
| GET | `/app/content-library/items/:id` | Get item details |
| DELETE | `/app/content-library/items/:id` | Delete an item |
| POST | `/app/content-library/items/:id/deploy` | Deploy from library item |

## Lifecycle

VM lifecycle policies and operations.

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/app/lifecycle/policies` | List lifecycle policies |
| POST | `/app/lifecycle/policies` | Create a lifecycle policy |
| PUT | `/app/lifecycle/policies/:id` | Update a lifecycle policy |
| DELETE | `/app/lifecycle/policies/:id` | Delete a lifecycle policy |

## Certificates

TLS certificate management.

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/app/certificates` | List certificates |
| POST | `/app/certificates` | Upload a certificate |
| DELETE | `/app/certificates/:id` | Delete a certificate |
| POST | `/app/certificates/:id/renew` | Renew a certificate |

## VPN Mesh

WireGuard-based VPN tunnels between VMs.

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/vpn-tunnels` | Create a VPN tunnel |
| GET | `/vpn-tunnels` | List VPN tunnels |
| GET | `/vpn-tunnels/:id` | Get tunnel details |
| PUT | `/vpn-tunnels/:id` | Update a tunnel |
| DELETE | `/vpn-tunnels/:id` | Delete a tunnel |
| POST | `/vpn-tunnels/sync` | Force tunnel reconciliation |
| GET | `/vpn-tunnels/status` | Get tunnel status |
| POST | `/vpn-networks` | Create a VPN network |
| GET | `/vpn-networks` | List VPN networks |
| GET | `/vpn-networks/:id` | Get network details |
| PUT | `/vpn-networks/:id` | Update a network |
| DELETE | `/vpn-networks/:id` | Delete a network |
| GET | `/vpn-networks/status` | Get network status |

### Example: Create a VPN Network

```
POST /api/vpn-networks
Content-Type: application/json

{
  "name": "dev-mesh",
  "selector": { "match_labels": { "env": "dev" } },
  "subnet": "10.10.0.0/24",
  "topology": "full_mesh"
}
```

**Response (201 Created):**
```json
{
  "id": "...",
  "name": "dev-mesh",
  "topology": "full_mesh",
  "subnet": "10.10.0.0/24",
  "enabled": true
}
```

## Packet Mirror

Traffic mirroring for VM debugging.

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/mirror-sessions` | Create a mirror session |
| GET | `/mirror-sessions` | List mirror sessions |
| GET | `/mirror-sessions/:id` | Get session details |
| PUT | `/mirror-sessions/:id` | Update a session |
| DELETE | `/mirror-sessions/:id` | Delete a session |
| POST | `/mirror-sessions/sync` | Force mirror reconciliation |
| GET | `/mirror-sessions/status` | Get session status |

## NAT Gateway

Advanced NAT: masquerade, SNAT, DNAT, hairpin.

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/nat-rules` | Create a NAT rule |
| GET | `/nat-rules` | List NAT rules |
| GET | `/nat-rules/:id` | Get rule details |
| PUT | `/nat-rules/:id` | Update a rule |
| DELETE | `/nat-rules/:id` | Delete a rule |
| POST | `/nat-rules/sync` | Force NAT reconciliation |
| GET | `/nat-rules/status` | Get rule status |
| POST | `/nat-pools` | Create a SNAT pool |
| GET | `/nat-pools` | List SNAT pools |
| GET | `/nat-pools/:id` | Get pool details |
| DELETE | `/nat-pools/:id` | Delete a pool |
| POST | `/nat-gateways` | Create a NAT gateway |
| GET | `/nat-gateways` | List NAT gateways |
| GET | `/nat-gateways/:id` | Get gateway details |
| DELETE | `/nat-gateways/:id` | Delete a gateway |

## Network Monitor

Per-VM bandwidth monitoring and alerting.

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/monitor-policies` | Create a monitor policy |
| GET | `/monitor-policies` | List monitor policies |
| GET | `/monitor-policies/:id` | Get policy details |
| PUT | `/monitor-policies/:id` | Update a policy |
| DELETE | `/monitor-policies/:id` | Delete a policy |
| POST | `/monitor-policies/sync` | Force monitor reconciliation |
| GET | `/monitor-policies/status` | Get policy status |
| GET | `/network-metrics` | Get all VM network metrics |
| GET | `/network-metrics/:name` | Get per-VM network metrics |
| GET | `/bandwidth-alerts` | Get active bandwidth alerts |

## Encryption

Data-at-rest and key management.

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/app/encryption/keys` | List encryption keys |
| POST | `/app/encryption/keys` | Create an encryption key |
| DELETE | `/app/encryption/keys/:id` | Delete an encryption key |
| POST | `/app/encryption/keys/:id/rotate` | Rotate an encryption key |
| POST | `/app/vms/:name/encrypt` | Encrypt a VM's disks |

## Resource Pools

Resource grouping and allocation.

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/resource-pools` | List resource pools |
| POST | `/resource-pools` | Create a resource pool |
| GET | `/resource-pools/:id` | Get pool details |
| PUT | `/resource-pools/:id` | Update a resource pool |
| DELETE | `/resource-pools/:id` | Delete a resource pool |
| POST | `/resource-pools/:id/assign` | Assign a VM to a pool |

## Datacenters

Logical datacenter management.

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/app/datacenters` | List datacenters |
| POST | `/app/datacenters` | Create a datacenter |
| GET | `/app/datacenters/:id` | Get datacenter details |
| PUT | `/app/datacenters/:id` | Update a datacenter |
| DELETE | `/app/datacenters/:id` | Delete a datacenter |

## Machines (removed)

The `/app/machines` machinectl / systemd-machined UI was removed. Use Virtual
Machines (`/app/vms`) and `/api/vms` (FluxVM).

## Events

System and VM event stream.

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/events` | Query events |
| GET | `/events/:id` | Get event details |
| GET | `/events/stream` | SSE event stream |

## Autoscale

Automatic VM scaling policies.

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/autoscale/policies` | List autoscale policies |
| POST | `/autoscale/policies` | Create an autoscale policy |
| GET | `/autoscale/policies/:id` | Get policy details |
| PUT | `/autoscale/policies/:id` | Update a policy |
| DELETE | `/autoscale/policies/:id` | Delete a policy |

## Hotplug

Live add/remove of devices to running VMs.

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/app/vms/:name/hotplug/cpu` | Add/remove CPUs |
| POST | `/app/vms/:name/hotplug/memory` | Add/remove memory |
| POST | `/app/vms/:name/hotplug/disk` | Attach/detach disk |
| POST | `/app/vms/:name/hotplug/nic` | Attach/detach network interface |

## Image Builder

Custom VM image creation.

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/image-builder/builds` | List builds |
| POST | `/image-builder/builds` | Start a new image build |
| GET | `/image-builder/builds/:id` | Get build status |
| DELETE | `/image-builder/builds/:id` | Cancel/delete a build |
| GET | `/image-builder/recipes` | List build recipes |
| POST | `/image-builder/recipes` | Create a build recipe |

---

## Migrations

Disk-copy path (`rsync` over SSH) and native FluxVM transport (preview). See [migration.md](migration.md) and [FLUXVM-FABRIC-BOUNDARY.md](FLUXVM-FABRIC-BOUNDARY.md).

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/migrations` | Start disk-copy migration (`live` / `offline` / `storage`) |
| GET | `/migrations` | List migrations |
| GET | `/migrations/{id}` | Migration status |
| POST | `/migrations/{id}/cancel` | Cancel a migration |
| GET | `/migrations/history` | Completed / failed history |
| GET | `/migrations/readiness` | SSH + rsync preflight |
| POST | `/vms/{name}/migration/native/prepare-receiver` | Arm target receiver (preview) |
| POST | `/vms/{name}/migration/native/start` | Start native transport (preview) |
| GET | `/vms/{name}/migration/native/status` | Native status (preview) |
| POST | `/vms/{name}/migration/native/cancel` | Cancel native (preview) |
| GET | `/vms/{name}/migration/native/network-state` | Dataplane migration phase |
| POST | `/migration/receivers/{id}/activate` | Activate receiver |
| DELETE | `/migration/receivers/{id}` | Abort receiver |

## VM Dataplane & QGA

Proxied FluxVM Network Fabric schema v4 and guest QGA. Operator detail: [fluxvm-dataplane.md](guides/vm-drivers/fluxvm-dataplane.md).

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/vms/{name}/dataplane/status` | Attach/schema; may include `pod_ingress_*` |
| GET/POST | `/vms/{name}/dataplane/policy` | Get / replace policy |
| GET | `/vms/{name}/dataplane/stats` | Counters; may include `pod_policy` |
| GET | `/vms/{name}/dataplane/flows` | LRU flows |
| GET | `/vms/{name}/dataplane/effective` | Merged effective policy |
| GET | `/vms/{name}/dataplane/drop-reasons` | Drop-reason histogram |
| GET/POST/DELETE | `/vms/{name}/dataplane/pod-policy` | Pod-ingress policy |
| GET/POST/DELETE | `/dataplane/groups[/{name}]` | Security groups |
| GET/POST/DELETE | `/dataplane/cnp[/{name}]` | CNP |
| GET | `/dataplane/{health,observe,identities,ipcache}` | Cluster dataplane |
| POST | `/dataplane/refresh-dns` | Re-resolve FQDN allowlists |
| POST | `/vms/{name}/live-fork` | Fork a *running* VM into 1-32 children from one memory snapshot (flux-vm KVM engine). `{"count": N, "name_prefix": "..", "ready": true, "idempotency_key": ".."}`; `ready` returns FluxVM's measured `first_command_ms` |
| GET | `/vms/{name}/memory` | VMM-process PSS/private/shared and balloon |
| GET/POST | `/vms/{name}/balloon` | Memory balloon (**Beta**: FluxVM lists it unit-tested, not live-verified; flux-vm KVM engine only). POST `{"balloon_mib": N}` is admin; 0 deflates Distinct from the older `/vms/{name}/memory/balloon` route, (enable/disable ballooning via the QEMU monitor), not FluxVM's balloon target |
| POST | `/vms/{name}/qga/ping` | QGA ping |
| POST | `/vms/{name}/qga/exec` | QGA exec |
| * | `/vms/{name}/qga/firewall/*` | QGA firewall helpers |

## Container Groups

Secure Containers placement (Kubernetes Pod + `RuntimeClass fluxvm`). See [container-groups.md](container-groups.md).

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/container-groups` | List groups |
| POST | `/container-groups/apply` | Apply a ContainerGroup |
| GET | `/container-groups/{name}/status` | Live status |
| DELETE | `/container-groups/{name}` | Delete |

## Agents & Sessions

Proxied to **agent-runtime** when `[agent_runtime]` is set; otherwise **503**. See [tutorials/11-agent-runtime-quickstart.md](tutorials/11-agent-runtime-quickstart.md). Harness runs, MCP, agent cron, signed webhooks, loops, approvals, and delegation are served by the agent-runtime process itself (`:9096`), not by these proxy routes. VM backup schedules under `/schedules` are unrelated.

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/agents` | List agents |
| GET | `/sessions` | List sessions |
| POST | `/sessions` | Create session |
| GET | `/sessions/{id}` | Session detail |
| POST | `/sessions/{id}/{hibernate\|resume\|cancel}` | Session actions |
| DELETE | `/sessions/{id}` | Delete session |

---

## WebSocket Endpoints

WebSocket connections require the same authentication token, passed as a query parameter or via the initial HTTP upgrade headers.

| Endpoint | Description |
|----------|-------------|
| `ws://host:8080/ws/console/:vmname` | Interactive terminal console (xterm.js) |
| `ws://host:8080/ws/vnc/:vmname` | VNC graphical console proxy (noVNC) |
| `ws://host:8080/ws/events` | Real-time event stream for live UI updates |

### Console Example

```javascript
const ws = new WebSocket("ws://localhost:9095/ws/console/myvm?token=<token>");
ws.onmessage = (event) => term.write(event.data);
term.onData((data) => ws.send(data));
```

---

## VM States

| State | Description |
|-------|-------------|
| `running` | VM is running |
| `stopped` | VM is stopped |
| `paused` | VM is paused |
| `starting` | VM is being started (async, returns 202 Accepted) |
| `stopping` | VM is being stopped |
| `failed` | VM encountered an error |
| `unknown` | VM state cannot be determined |

## Error Responses

All errors return a JSON body:

```json
{
  "error": "Error message here",
  "code": "ERROR_CODE"
}
```

**Common Status Codes:**

| Code | Meaning |
|------|---------|
| 200 | OK |
| 201 | Created |
| 204 | No Content |
| 400 | Bad Request |
| 401 | Unauthorized |
| 403 | Forbidden |
| 404 | Not Found |
| 409 | Conflict |
| 422 | Unprocessable Entity |
| 429 | Too Many Requests |
| 500 | Internal Server Error |
