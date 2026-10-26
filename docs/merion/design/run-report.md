# Design: a run report (what was checked, what remains, how to roll back)

Status: **built (JSON endpoint), with differences from the plan below.** `GET /v1/sessions/{id}/report`
(`agent-runtime/src/run_report.rs`). Tested with 11 unit tests and an end-to-end test over the real approve, deny and failed-apply
flows against the FluxVM stub; not yet run against a real FluxVM host.

**Differences from the plan:**
- Step 0, corrected: FluxVM does have tagged snapshots and an in-place restore (`POST /v1/vms/{id}/snapshot` and `/restore`, used by the Fabric
  client). My first answer was wrong because I only read the runtime's own client, which had no restore call. Now built, opt in per agent:
  `snapshot_before_apply` takes a snapshot just before an approved changeset is applied, and an **operator** can restore it with
  `POST /v1/sessions/{id}/changesets/{cs}/restore {"confirm": true}`. A restore returns the *whole cell* to that moment (files, memory, the
  agent process), so it is refused unless the change is the latest applied one, a snapshot exists, and it was not already restored.
  Users cannot call it. Rollback states: `nothing_to_undo`, `undo_available`, `restored_from_snapshot`, `applied_no_automatic_undo`,
  `unknown`, `cannot_be_undone`. Tested against a fake FluxVM only; not run on a real cell.
- The verify command is built: `verify_command` in the manifest runs in the cell after the apply (120 s limit) and appears as a check,
  "the verify command exited 0", with the command and the last 5 output lines (each cut to 200 characters; output can echo file content, so
  keep it to a test command). It cannot be set by an agent. A command that cannot run is a failed check, not a missing one.
- The agent's own claims are not shown at all (not even labelled); the report lists "anything the agent said" under *not checked*.
- `merionctl report <session-id> [--json]` is built (a plain-text rendering of the same JSON).
- The console shows a **Run report** card on the session page (served by `GET /api/sessions/{id}/report`).
- Not built: the PDF section (there is no session PDF generator to extend; the console card can be printed).

## The problem

When an agent finishes, a person sees a stream of events, an approval for each risky step, and a changeset diff. Nothing
says, in one place: *what did the agent do, what was actually verified, what was not, and how do I undo it?* Products in
this space promise exactly that sentence at the end of a run. Merion holds almost all of the facts already; it does not
put them together.

The danger is the opposite one: a friendly summary that sounds more certain than the evidence. A report that says
"verified" when nothing was checked is worse than no report. So the design starts from what the host can **measure**.

## What exists today (checked against the code)

- **Changesets** (`agent-runtime/src/fluxvm.rs`, `Changeset`): `state` (`pending`, `approved`, `applied`, `rejected`,
  `expired`, `failed`), `exit_code`, `changes`, `side_effects`, `staged` with sizes, `unstaged` files, `error`.
- **Approvals** with who decided, when, and (for auto-approve) the policy id and version in the comment.
- **Receipts** ([receipts](../receipts/README.md)): each approved outbound action with method, URL without query, body
  size and SHA-256, approval id and the upstream status. Never the body.
- **The audit journal**, including `model.call`, `changeset.policy` and `merion.action.performed` rows.
- **Snapshots**: `FluxVmClient::snapshot` exists (`fluxvm.rs`). I have not checked whether Merion offers a restore from one
  to a user; treat that as unknown until step 0.

## Proposal

A **run report** is assembled by the host, from those records, when a session ends (or on demand). It has three fixed
sections, and each line carries its source so a reader can check it.

### Did

Chronological, one line per fact the host recorded: changesets (applied or rejected, files and bytes), actions performed
(from receipts), model calls (count, endpoint, no text), approvals (who, how: person, phone-signed, or auto by policy).

### Checked and not checked

This is the section that must not flatter. Only these count as "checked":

| Check | Source |
|---|---|
| The command exited 0 | changeset `exit_code` |
| Every changed file was kept | changeset `unstaged` is empty |
| The upstream accepted an action | receipt `status` 2xx |
| The change is what was approved | the applied changeset id matches the approved one |

Everything else is listed under **not checked**, by name: "the content of the files was not tested", "whether the email was
read was not observed". An agent's own claim ("tests pass") appears only as *the agent said*, in quotes, labelled as
unverified, and is never counted as a check. The host does not run the agent's tests; an optional **verify command**
(below) can.

### Remaining and rollback

- **Remaining:** items the host knows are open: approvals still pending, changesets not yet decided, the goal steps not
  finished (if the run was a goal run).
- **Rollback**, per item, in one of three honest states:
  - *Nothing to undo*: the changeset was rejected or expired, so nothing was applied.
  - *Undo available*: a file change applied in the cell, where a pre-run snapshot exists (step 0 decides whether this is
    offered).
  - *Cannot be undone by Merion*: any outbound action that left the cell (email sent, payment, a POST). The report says
    so plainly and names the receipt. It never invents an undo for these.

### Optional: a verify command

A pack may name a read-only check (for example a test command) that the **host** runs in the cell after the changeset is
applied, and whose exit code and last lines of output appear in the report. It is declared in the manifest and is part of
what a person approves with the pack, so an agent cannot add one at run time. It runs under the cell's existing egress
policy.

## Where it shows

`GET /v1/sessions/{id}/report` (JSON, same auth as the session), `merionctl report <session>`, a tab on the session page
and the PDF brief that already exists. Stored with the session so it survives a runtime restart. Never contains file
contents, request bodies, or secrets: only ids, paths, sizes, digests, statuses and counts, like receipts.

## What I would refuse to build

- A report written by the model, or any line taken from agent output presented as a host fact.
- A "verified" or "success" label that is not tied to one of the checks in the table above.
- A rollback button for an action that has left the cell.
- A verify command an agent can set.

## Plan

| Step | What | Test |
|---|---|---|
| 0 | Find out what undo really exists: is there a user-reachable restore, and does a pre-run snapshot cost enough to take by default? Document it | live run on FluxVM, or the stub with the gap stated |
| 1 | Report assembly from changesets, approvals, receipts and audit rows; JSON endpoint; `merionctl report` | a run with one applied and one rejected changeset; an email action; an agent claim that stays "unverified"; an empty run |
| 2 | Console tab and PDF section | browser check |
| 3 | Verify command in the manifest | a failing check shows as failed, not omitted |

## Decisions for you

1. **Snapshot before every run?** It makes "undo available" real but costs time and disk. Recommend: opt in per agent.
2. **Verify command in the first release?** Recommend no: ship steps 0 to 2, since the report is useful without it.
3. **Generated on session end, or only on request?** Recommend both: built at end, rebuilt on demand if records changed.
