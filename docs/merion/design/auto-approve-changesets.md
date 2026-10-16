# Design: auto-approve low-risk changesets

Status: **stage two built.** An operator can set a policy for one user's agent, and a changeset inside it is
approved without a click. Off by default: with no policy, nothing changes. Stage one (the dry-run journal) still exists
and is independent.

**Stage two as built**
- `PUT/GET/DELETE /v1/users/{id}/changeset-policies/{agent}` and `GET .../changeset-policies`. Writes are **operator
  only**, like device enrolment, so a stolen user, session or worker token cannot widen what skips a click. A user can
  read their own list. The body is the stage-one policy; the store assigns `version` (previous + 1) and journals each
  change (`merion.changeset_policy.set`, `.removed`).
- Scope is **per user and per agent** (not per session). A session with no user is never auto-approved.
- Hook: in `speculate_session`, after the approval is saved. The same `transition_approval` and `settle_changeset` a
  person's approve uses; the approval's `comment` reads `auto-approved by policy <id> v<n>` and the audit row has
  `auto: true`. A refusal writes a `changeset.policy` row with the reason.
- **A required phone signature wins**: when `ZYVOR_AGENT_REQUIRE_DEVICE_SIGNATURE=1` or the approval's credential
  requires one, nothing is auto-approved.
- Cap: 30 auto-approvals per session per hour, then a person decides.
- Not built: a tenant-wide ceiling, a console section, and a distinct `decided_by` field
  (the comment prefix is the marker for now).
- Tested with `scripts/merion-e2e.sh` against the FluxVM stub (18 checks: no policy waits; a user token cannot set one;
  bad policy 400; inside the policy runs approve then apply on FluxVM; a tightened policy leaves it pending; delete).
  Not run on a real FluxVM host.

Stage one (`agent-runtime/src/changeset_policy.rs`): a pure evaluator plus a journal entry. If
`ZYVOR_AGENT_CHANGESET_POLICY_DRYRUN_FILE` names a JSON policy file, every speculate writes an audit entry
`changeset.policy.dry_run` saying whether that policy *would* have approved, with the reason when not. The
approval stays pending either way. The file is read per request, so editing or deleting it takes effect at
once. It is set by the operator, not by an agent or a pack. Policy shape:
`{"id","version","paths":["/work"],"kinds":["added"],"max_files":20,"max_bytes":100000,"deny_globs":[],"require_exit_code":0}`.

Answered while building: FluxVM does report per-file size (`staged[path].size`), so `max_bytes` is usable; a
file with no reported size disqualifies when `max_bytes` is set. Unknown change kinds disqualify. Still open
for stage two: per-user policy storage and routes, per-agent versus per-session, the tenant ceiling, and the
signed-approval override.

## The problem

`POST /v1/sessions/{id}/speculate` always ends in a `changeset` approval that a person settles
(`speculate_session`, then `decide_approval` -> `settle_changeset` in `agent-runtime/src/app.rs`). That is the
right default, but a session that speculates often (a formatter, a notes file under `/work`) turns every run
into a click. People will either stop reading the diff or stop using speculation.

The aim is narrower approvals, not fewer guarantees: let a person pre-authorise a *named, bounded* class of
change, and keep everything else a human decision.

## Non-negotiables

1. **Off by default, and only a person can turn it on.** A policy is created and changed by an authenticated
   user (or operator) with the `approve` scope. Neither the agent nor the worker nor a pack manifest can
   create, edit or widen one. This is the same line `design/agent-tools.md` draws ("an agent that edits its
   own manifest... I would refuse to build").
2. **Fail closed.** Any field missing or unparseable, any unknown change kind, any evaluation error: the
   changeset stays `pending` for a person. Auto-approval only ever *adds* a decision path; it never turns an
   error into an approval.
3. **The diff is the evidence, not the command.** A policy matches on what FluxVM *measured* in the changeset
   (`changes`, `side_effects`, `unstaged`, `exit_code`), never on the command text, which the agent controls.
4. **Auditable and reversible by the owner.** Every auto-decision is journaled with the policy id and version,
   and is distinguishable from a human decision everywhere it is shown.

## What a policy may match

A policy is a conjunction; every clause must hold. There is no "or" and no expression language, so it can be
read and reasoned about in one sitting.

| Clause | Meaning | Default if absent |
|---|---|---|
| `paths` | Every changed file is under one of these guest directories | policy invalid (required) |
| `kinds` | Allowed change kinds from `added`, `modified`, `deleted` | `["added"]` only |
| `max_files` | Upper bound on `added+modified+deleted` | 20 |
| `max_bytes` | Upper bound on total changed size, if FluxVM reports it | policy invalid if the field is not reported |
| `deny_globs` | Never auto-approve if any changed path matches (`**/.ssh/**`, `**/*.pem`, `**/.env*`, dotfiles) | built-in list, not removable, only extendable |
| `require_exit_code` | Command exit code must equal this | `0` |

A changeset is **never** eligible, whatever the policy says, when any of these hold:

- `unstaged` is non-empty (FluxVM already refuses to apply; do not paper over it).
- `side_effects.egress` is set, or `side_effects.non_replayable` is non-empty. A file write that also sent
  something is not "just a file change".
- The session is tainted (`tainted_by` non-empty). Tainted input is exactly when a human should look.
- The session is not `Running`, or the approval has already expired.
- `deleted` appears and the policy did not list `deleted` explicitly (see `kinds` default above).

## Where it hooks in

One place: right after the approval is saved in `speculate_session`, before it is returned.

```
speculate -> changeset(pending) -> save approval -> evaluate(policy, changeset, session)
    eligible   -> transition_approval(Approved, decided_by = policy) -> settle_changeset (approve + apply)
    ineligible -> leave pending (today's behaviour)
```

Reuse, do not fork: the auto path calls the same `transition_approval` and `settle_changeset` so state
transitions, audit shape and the FluxVM approve-then-apply sequence stay identical. The evaluator is a pure
function `fn eligible(policy, changeset, session) -> Result<(), Reason>` so it is unit-testable without
FluxVM or a store, and `Reason` is returned to the caller and written to the audit entry on refusal too
(a person asking "why did this need me?" gets an answer).

`decide_approval` is untouched. It still requires a principal, and `crate::devices::check_decision` still
applies to human decisions. Auto-approval is a separate server-side path, not a way to submit a decision.

## Audit and display

- Audit action stays `approval.changeset`, phase `Approved`, with extra detail
  `{auto: true, policy_id, policy_version, matched: {...}, files: n}` so existing consumers keep working.
- The approval record carries `decided_by: "policy:<id>"` (new optional field; absent means human) and the
  console shows "Auto-approved by policy X" with the diff still visible, not collapsed.
- The Outcome card and `merionctl approvals` list a distinct status label so an auto decision is never
  mistaken for a signed human one.
- Rate limit: at most `N` auto-approvals per session per hour (default 30). Past that, changesets fall back
  to `pending` and a single audit entry says why. This bounds the damage of a looping agent.

## Threat model additions

| Abuse | Mitigation |
|---|---|
| Agent writes a harmless file, then widens its own policy | Policies are not writable with a session/worker token; only a user token with `approve` scope and, where configured, a device signature (`keep-approval-v1`) |
| Agent hides sensitive output under an allowed path | `deny_globs` is a floor that policy cannot remove; `max_files`/`max_bytes` bound blast radius; egress or non-replayable side effects disqualify |
| Many tiny auto-approved writes add up to something large | Per-session hourly cap; audit lists every one |
| Policy matches by command string ("looks like echo") | Not a clause. Command is shown but never evaluated |
| Stale policy keeps approving after the user changes their mind | Revoke is immediate; evaluation reads the policy at decision time, never caches it past the request |
| Tainted session launders content onto disk | Taint disqualifies auto-approval outright |

Honest limit: auto-approval does **not** make the file content safe. It only skips the click for changes the
owner pre-judged acceptable by location, kind and size. A file under an allowed path can still contain
something the owner would not want. The policy is a convenience for a class, not a verdict on the bytes.

## Where policies live

Proposal: a `changeset_policies` collection in the runtime store, keyed by `(user_id, agent)`, with an
integer `version` bumped on every edit (journaled). Managed through:

- `POST/GET/DELETE /v1/users/{id}/changeset-policies` (runtime), proxied by fabricd with the same ownership
  checks as sessions;
- `merionctl policy changeset ...` and a section on the session page of the console.

Not in the agent manifest or pack, so a signed pack cannot ship its own permission to skip approval.

## Unknowns to settle before building

- Does FluxVM's changeset report per-file size? `max_bytes` is only usable if it does. If not, drop the
  clause rather than approximate. Check `tests/fixtures/fluxvm-changeset.json` and a live run.
- Exact shape of `changes` for renames and mode-only changes. Unknown kinds must disqualify (non-negotiable 2).
- Should a policy be per-agent or per-session? Per-agent is less clicking; per-session is safer. Start
  per-agent with an optional session pin.
- Do operators need a tenant-wide ceiling above user policies (as `agent-tools.md` does for tool authority)?
  Likely yes; reuse the same ceiling concept rather than inventing a second.
- Phone-signed approvals: a user who requires device signatures for approvals may not want auto-approval at
  all. Proposal: a user-level switch "require a signed decision for every changeset" that overrides policy.

## Verification plan (when built)

Against the stub (`agent-runtime/tests/sandbox_stub.py`) and one live run:

1. Eligible changeset under an allowed path is approved and applied without a human; audit shows `auto: true`.
2. Each disqualifier above (unstaged, egress, non-replayable, taint, deny-glob, over `max_files`, non-zero
   exit, deleted-not-allowed) leaves the approval `pending`, with the reason in the audit.
3. A session token cannot create or edit a policy (403/404); another user cannot see it.
4. Malformed or missing `changes` fails closed.
5. Revoking a policy takes effect on the very next speculate.
6. The hourly cap trips and falls back to a human.

## Recommendation

Build it last, after batches and the expiry UX, and only if real use shows the clicks are a problem. If
built, ship the evaluator and audit first behind a config flag in "dry run" mode (log what *would* have been
auto-approved, approve nothing) so the owner can see the policy's behaviour on real changesets before it can
act.
