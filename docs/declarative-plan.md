# Declarative apply plan (dry run)

`POST /api/vms/apply` takes a declarative `VMSpec`. A plan shows what that apply
would do before you run it, in the style of `terraform plan`. Nothing is
changed by planning.

## What a plan contains

Each VM gets one entry, with an action:

| Action | Meaning |
|---|---|
| `create` | The VM does not exist. Every field is listed with `before: null`. |
| `update` | An apply would change at least one field. Changed fields carry `before` and `after`. |
| `noop` | An apply would change nothing. |
| `delete` | Only when `prune: true`: the VM exists but is not in the desired set. |
| `unmanaged` | The VM exists, is not in the desired set and `prune` was not set. Reported, never deleted. |

Entries are ordered create, update, delete, noop, unmanaged, then by name, so
the same inputs always give the same output. `summary` holds the counts,
`has_changes` is true when anything would be created, updated or deleted, and
`plan_hash` is a SHA-256 (hex) over the canonical plan.

The plan mirrors what the apply really reconciles for an existing VM: start
options, volumes (only when the spec lists some), autoscale (only when the spec
sets it) and direct-uplink fields (only when the spec sets an uplink). Drift in
creation-only fields (image, cpus, memory, disk, hostname, tags) is listed with
`applied: false`; it does not make an entry an `update`, because apply would not
change it. Memory and disk are compared after unit normalisation (`2G` equals
`2048`).

Secret-looking values (keys such as password, secret, token, api_key, the
`value` of credential entries, and token-shaped strings like `ghp_...`, JWTs and
PEM blocks) are shown as `[REDACTED]`. The `plan_hash` still covers a digest of
the real values, so rotating a secret changes the hash without exposing it.

## API

```
POST /api/vms/apply?dry_run=true      body: VMSpec          -> plan (single VM)
POST /api/vms/apply?expected_plan_hash=<hash>  body: VMSpec -> apply, or 409
POST /api/vms/plan                    body: {"vms": [VMSpec, ...], "prune": false}
```

- `dry_run=true` returns the plan for that one VM and changes nothing. It needs
  the same role as a real apply. The call is audit-logged as a `read` on
  `vm_apply_plan`.
- `expected_plan_hash` is optional. If present and the live plan for the spec
  hashes differently (the VM changed since you reviewed the plan), the apply is
  refused with `409` and the body holds `expected_plan_hash`, `actual_plan_hash`
  and the new `plan`. Without it, apply behaves exactly as before. The check is
  not atomic with the apply itself.
- `POST /api/vms/plan` plans a whole set against every VM on the host and needs
  only read access. With `"prune": true`, VMs not in the set appear as `delete`.
  Apply itself is per VM and never deletes, so `delete` entries are advisory
  today.

A dry run does not run create-time validation or the quota check; those still
happen on the real apply. `auto_start: true` is not part of the plan: a real
apply will try to start the VM regardless.

## CLI

`fabricctl apply -f <file>` reads `kind` from the file. The plan flags work for
`kind: vm-spec`, which targets `POST /api/vms/apply`:

```yaml
kind: vm-spec
spec:
  name: web-ci
  image: /var/lib/zyvor-fabricd/images/ubuntu-22.04.qcow2
  resources: { cpus: 2, memory: 2G }
```

```
fabricctl apply -f vm.yaml --dry-run                      # print the plan
fabricctl apply -f vm.yaml --dry-run --detailed-exitcode  # exit 0 = no changes, 2 = changes pending
fabricctl apply -f vm.yaml --expected-plan-hash <hash>    # apply only if the plan is unchanged
```

`--detailed-exitcode` requires `--dry-run`; `--expected-plan-hash` cannot be
combined with `--dry-run`. For any other `kind` these flags are rejected.
With `-o json` or `-o yaml` the raw plan is printed.
