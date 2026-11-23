# VM configuration history and "what changed since last known good"

Answers three questions per VM: who changed its configuration, when, and what is different now compared with the last time it worked. Read-only for the VM: nothing here changes, restarts or reverts a VM. Research and rejected ideas: [RESEARCH-config-history.md](RESEARCH-config-history.md).

## What is recorded

A revision is a copy of the VM's configuration: the VM record (image, CPUs, memory, disk, hostname, tags, labels, MAC, networking, port forwards, cloud-init, storage) plus its boot, CPU model, display, watchdog, serial, volume and start-option documents. Runtime state (power state, IP, pid, VNC port, timestamps, last error) is not part of it.

A revision is written when the configuration changes through: create, clone, template deploy, declarative apply, tag add/remove/replace, port-forward add/remove, direct-uplink set/clear, cloud-init, and boot, display, CPU model, watchdog and serial edits. Writing the same configuration again records nothing (the content hash is unchanged). Changes that bypass those handlers (for example a disk resize, or an edit to the store) are caught when the history is next read and recorded as `system:observed`; the actor of those changes is unknown. The first read of a VM that existed before this feature records a `baseline`.

Each revision stores: number, time, actor, cause (the action), SHA-256 `content_hash` of the stored document, `prev_hash` and `rev_hash` (revisions chain per VM), and the id, `seq` and `hash` of the audit event it wrote.

## Chained to the audit log

Every revision writes a `VM_CONFIG_REVISION` event with the audit trail's own writer, so it is hash-chained when `[audit] chain = true` (see [audit-export.md](audit-export.md)). Its details hold the revision number, content hash and cause, never configuration values. `GET .../history` reports each revision's `audit.link`: `ok` (event found, hash matches), `unchained` (chain off), `mismatch`, or `missing_or_pruned` (the event was removed, for example by audit retention). The revision chain itself is checked on every list: `chain.status` is `ok`, `broken` (with the first revision and reason: `rev_hash_mismatch`, `content_hash_mismatch`, `prev_hash_mismatch`, `revision_missing`, `head_mismatch`, `truncated`) or `empty`. Like the audit chain, the hashes are unkeyed: they detect edits and deletions, not a rewrite of the whole store.

## What is never stored

Secrets are not stored in clear, in revisions, API output, audit events or logs:

- values under keys that look secret (password, passwd, passphrase, secret, token, credential, private, key, api key, cookie, auth, pass) at any depth;
- cloud-init `runcmd`, write-file `content`, `user_data`, kernel arguments, environment and script fields (free text that may embed secrets);
- SSH public keys, private key blocks and JWT-like strings wherever they appear;
- inline `password=...`, `token: ...`, `secret=...` assignments inside any string, and the password part of `scheme://user:pass@host`.

They are replaced by `[redacted:<12 hex>]`, a SHA-256 prefix salted with a random per-install value (`config_history_meta/salt` in the state store). Equal values give equal markers, so a diff can say "changed (value hidden)" without showing anything. Limits, stated plainly: a short or guessable secret could be guessed by someone who holds both the store and the salt; the scan is by key name and simple patterns, so a secret in an ordinary field with an ordinary name can be stored (for example in a free-text label value without `password=` in front of it). Do not put secrets in labels or tags.

## Bounds

Strings are cut at 256 characters (the cut text carries a hash of the full text, so a change past the cut still changes the revision), arrays at 64 items, objects at 128 keys, depth at 8, keys at 64 characters; control and bidirectional-override characters are replaced; a document still larger than 64 KiB after that is stored as a size and hash per top-level section (`_oversize`). Revisions that were cut have `truncated: true`. Actor and cause are cleaned and limited to 128 and 160 characters.

## Retention

`[config_history] max_revisions` (default 100, range 5 to 1000) per VM. The oldest revisions go first; the newest revision and the last known good revision are never evicted. Eviction is recorded (`retention.evicted_total`) and does not count as tampering. Revisions of a deleted VM stay until removed by hand (`vm_config_history/<vm>` in the state store).

## Last known good

A revision is marked good when either:

- **auto**: after the revision, the VM stayed in the Running state for `[config_history] good_after_minutes` (default 15, range 1 to 1440) according to its lifecycle events (started, resumed or auto-healed, with no stop, pause, error, created or deleted event in between), and is Running without a recorded error now. The window for a revision ends at the next revision. The check runs when the history list, a diff or since-good is read, not on a timer, so a mark can appear later than the moment the VM qualified.
- **operator**: `POST /api/vms/{name}/history/{rev}/mark-good` (write access, audited as `VM_CONFIG_MARK_GOOD`, optional `{"note":"..."}`).

Honest limit: the automatic signal is VM lifecycle state, not guest or application health. A VM that is Running but whose application is broken still earns a good mark. Fabric has no reliable in-guest health signal for every VM; if you have one, use `mark-good`. A VM with no recorded lifecycle events (created before events existed) never gets an automatic mark.

## API

All paths are under `/api`. Read access for GETs; write access for `mark-good`. Tenant-scoped tokens only reach their own VMs. Revision numbers are positive integers; a bad number is 400, an unknown or evicted revision is 404.

| Method | Path | Returns |
|---|---|---|
| GET | `/vms/{name}/history?limit=` | Revisions newest first (default 50, max 200) with actor, cause, hashes, audit link, good mark, number of changed fields and high-risk changes against the previous revision; `current_rev`, `last_good_rev`, retention, chain status, the good-signal explanation |
| GET | `/vms/{name}/history/{rev}` | One revision with its redacted configuration |
| GET | `/vms/{name}/history/diff?from=&to=` | Field-level diff (`to` defaults to the newest revision) |
| GET | `/vms/{name}/history/since-good` | Diff from the last good revision to now |
| POST | `/vms/{name}/history/{rev}/mark-good` | Mark a revision good |

The list, diff and since-good reads first record the live configuration if it differs from the newest revision (see above), so these GETs can write one `system:observed` revision and audit event.

### Diff

Each change has `path`, `label`, `kind` (`added`, `removed`, `changed`), `from`, `to`, a one-sentence `summary` in operator language, `hidden` (value is a redaction marker) and an optional `risk` with `level` (`high`, `medium`, `low`) and a `reason`. Lists of plain values (tags, SSH keys, packages) are compared as sets; lists of objects (volumes, port forwards) by position. Identical revisions give an empty change list and `identical: true`. Flagged as risky:

| Change | Level |
|---|---|
| vCPU count, memory or disk size decreased | high |
| Disk or volume removed, disk bus changed | high |
| NIC MAC address changed, NIC model changed, NIC removed | high |
| Firmware, machine type, CPU model/mode/vendor, storage backend changed | high |
| Boot order, Secure Boot, direct-boot kernel/initrd/arguments, CPU feature lists, base image, network attachment, port forward removed or changed, host-association label changed | medium |
| SSH keys, cloud-init content, hostname, eviction label | low |

Only fields Fabric actually stores are compared. Fabric does not store a disk bus, NIC model or machine type for a VM today, so those rules apply to documents that carry such fields (they are in place for them) and are covered by tests on the rule paths, not by a live VM.

### Since good

`status` is `no_known_good` (with advice), `unchanged`, or `changed`. When changed: `summary` (one paragraph: which revision was good and how, how many revisions by whom, how many fields differ, how many high and medium risk, and the most likely culprit with its revision and actor), `counts`, and `culprits` ranked: risk level first, then the most recent change first. Each culprit lists `changed_in`: the revisions after the good one that touched that field, with actor, time and cause.

## CLI

```
fabricctl vm history web-1 [--limit N] [--rev N]
fabricctl vm diff web-1 --from 2 [--to 5]
fabricctl vm since-good web-1
fabricctl vm mark-good web-1 3 [--note "checked by hand"]
```

Table output by default; `-o json` prints the raw response.

## Web

VM detail, History tab: what changed since last known good with ranked culprits, the revision table (pick two to compare, mark a revision good if you can write), and a broken-chain warning.

## Revert plan

Not provided. There is no auto-revert and no generated revert commands: to undo a change, apply the old value through the normal VM settings. The diff gives the old value, except for hidden (redacted) values, which cannot be recovered from history by design.

## Configuration

```toml
[config_history]
max_revisions = 100
good_after_minutes = 15
```
