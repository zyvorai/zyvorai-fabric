# Design: branch a Keep session with FluxVM fork

Status: **proposal, nothing implemented.** It exists so the decision is made on the facts, not on the
demo appeal of "16 agents from one".

## What it would give

`POST /v1/sessions/{id}/branch {count}` would start `count` new Keep sessions whose cells are FluxVM
forks of the parent's running cell, so each begins with the parent's memory and disk state instead of
cold-booting and replaying setup. Use: try several approaches from one prepared state, then keep one.

FluxVM already forks a running VM (`POST /v1/vms/{id}/fork`, `fork_vm` in `fluxvm-client`, run live on
KVM). Keep already tracks lineage (`parent_session_id`) and already hibernates and resumes cells from
snapshots (`hibernate_session`, `resume_session`). Branching sits between the two.

## What a fork copies, and why that is the hard part

A forked cell is a byte-for-byte copy of a running machine. For a Keep cell that includes things Keep
deliberately makes unique per session:

| Copied into the child | Why it is a problem |
|---|---|
| The worker's `ZYVOR_SESSION_ID` and `ZYVOR_EGRESS_CAPABILITY` (set in the launch command, `app.rs`) | The child would present the **parent's** capability token to the egress broker. Two cells, one identity: audit, approvals, taint and policy decisions would be attributed to the wrong session. |
| Surrogate tokens and the MITM CA for intercepted credentials | Surrogates are derived from the session's capability token. The child holds the parent's surrogates. |
| The guest's IP, MAC and network policy | Two live cells with one address. FluxVM's per-VM network policy would have to be re-applied to the child and the guest renumbered. |
| Anything in guest memory: tokens, decrypted files, an in-progress browser profile | A branch of a tainted or secret-holding session is tainted too, and the branch must inherit `tainted_by`. |
| The in-flight worker state (a half-finished tool call) | The child would resume mid-call. Side effects already sent (an email, a purchase) would be replayed or desynchronised. |

So a branch is **not** safe to hand to an agent as-is. It needs, at minimum:

1. A new session record with a **fresh capability token**, and a way to give it to the running child
   worker (the worker only reads it at launch). That needs a worker-side re-key endpoint or restarting
   the worker inside the fork; both change the guest contract.
2. A fresh network identity and policy for the child, applied before the child can run.
3. `tainted_by`, approvals-in-flight and the audit chain carried over: parent history copied, child
   events appended, lineage recorded.
4. A rule for pending approvals and non-replayable side effects at fork time. Simplest honest rule:
   refuse to branch while the parent has a pending approval or an unfinished tool call.
5. The same limits as everything else: `max sessions per agent/user`, and `count` bounded (FluxVM allows
   1-32).

## What is unknown, and must be measured on a real host first

- Whether a forked VM, which FluxVM exposes under `/v1/vms`, can be driven through `/v1/sandboxes/{id}/...`
  (Keep's `process`, `fs`, `guest_request`) the way a Keep cell is. Speculation does fork internally, but
  that is FluxVM's own path, not a child Keep can address.
- Whether the guest agent and the worker survive a fork cleanly (open sockets, vsock connections).
- Real fork cost on a host with reflinks. The only fork run so far took ~106 s for 2 children on a
  filesystem without reflinks, copying a 2.2 GB image. That number says nothing about a tuned host, and
  no speed claim should be made until it is measured on one.
- Whether the Keep runtime's own cell security profile (software-test, hardware attestation) can be
  claimed for a fork. Likely not: say so in the session's honesty badge.

## Recommendation

Do not build this yet. Order of work if it is wanted:

1. On a host with a current FluxVM and reflinks, run a spike: fork a Keep cell by hand, then try
   `process`/`guest_request` on the child and measure the real time. Record the results here.
2. Decide the re-key approach (worker restart inside the fork is the least invasive).
3. Only then add `POST /v1/sessions/{id}/branch`, refusing when the parent has a pending approval, with
   e2e coverage against the stub and a live run before any claim goes in the README.

Until then the README and Pages must not describe Keep as branching sessions. `fabricctl fork` (a plain
VM fork) is shipped and is a different, simpler thing.
