# Design: branch a Merion session with FluxVM fork

Status: **proposal, nothing implemented; a first spike (below) found that QEMU-backed Merion cells cannot be forked.** It exists so the decision is made on the facts, not on the
demo appeal of "16 agents from one".

## What it would give

`POST /v1/sessions/{id}/branch {count}` would start `count` new Merion sessions whose cells are FluxVM
forks of the parent's running cell, so each begins with the parent's memory and disk state instead of
cold-booting and replaying setup. Use: try several approaches from one prepared state, then keep one.

FluxVM already forks a running VM (`POST /v1/vms/{id}/fork`, `fork_vm` in `fluxvm-client`, run live on
KVM). Merion already tracks lineage (`parent_session_id`) and already hibernates and resumes cells from
snapshots (`hibernate_session`, `resume_session`). Branching sits between the two.

## What a fork copies, and why that is the hard part

A forked cell is a byte-for-byte copy of a running machine. For a Merion cell that includes things Merion
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
  (Merion's `process`, `fs`, `guest_request`) the way a Merion cell is. Speculation does fork internally, but
  that is FluxVM's own path, not a child Merion can address.
- Whether the guest agent and the worker survive a fork cleanly (open sockets, vsock connections).
- Real fork cost on a host with reflinks. The only fork run so far took ~106 s for 2 children on a
  filesystem without reflinks, copying a 2.2 GB image. That number says nothing about a tuned host, and
  no speed claim should be made until it is measured on one.
- Whether the Merion runtime's own cell security profile (software-test, hardware attestation) can be
  claimed for a fork. Likely not: say so in the session's honesty badge.

## Spike results (2026-11-03, lab host, FluxVM 127.0.0.1:7788)

Step 1 of the order of work below was started on the lab host. What it showed, and what it did not:

- **A Merion cell on the QEMU backend cannot be forked.** A `keep-cdp-stub` session (template `node22-agent`) was running, and
  `POST /v1/vms/{id}/fork?ready=exec` with `{"count":1,"namePrefix":"spike"}` came back in 0.4 s with
  `400 {"error":"fork supports the flux-vm backend only (backend=Qemu)"}`. No child was created. So branching needs the cell to be
  on FluxVM's own backend, which the shipped Merion templates on this host are not. The earlier ~106 s fork of 2 children was a
  plain `flux-vm` VM, not a Merion cell.
- **The copied-identity table above is confirmed for the parent.** Inside the running cell, the worker's environment holds
  `ZYVOR_SESSION_ID=<the session id>`, the guest has one global address (`169.254.0.3/16`) and a `boot_id`. A fork would copy all
  three, as the table says.
- **Not answered.** Whether a forked VM can be driven through `/v1/sandboxes/{id}/process`; whether the guest agent and worker
  survive a fork; the fork cost on a host with reflinks (this host is ext4 without reflinks, so it could not say); and whether a
  flux-vm-backend Merion cell exists to try. The only template that might be on that backend, `node22-fc` (agent `keep-fc`), runs
  one command and completes, so there was never a running cell to fork. A long-running Merion agent on `node22-fc` is the missing
  piece for the next attempt.
- **A side observation, not about forks.** One of three session starts that day failed with FluxVM
  `400 ... bpftool prog load ... can't pin BPF object: path '/sys/fs/bpf/fluxvm/vms/<id>/progs/fluxvm_egress' already exists`
  while applying the cell's network policy. The next start worked and nothing was left pinned afterwards. It looks like the
  policy being applied twice for one VM; that is a guess, not a finding.

The recommendation is unchanged, and now has a first fact behind it: do not build branching on the QEMU cells.

## Recommendation

Do not build this yet. Order of work if it is wanted:

1. On a host with a current FluxVM and reflinks, run a spike: fork a Merion cell by hand, then try
   `process`/`guest_request` on the child and measure the real time. Record the results here.
2. Decide the re-key approach (worker restart inside the fork is the least invasive).
3. Only then add `POST /v1/sessions/{id}/branch`, refusing when the parent has a pending approval, with
   e2e coverage against the stub and a live run before any claim goes in the README.

Until then the README and Pages must not describe Merion as branching sessions. `fabricctl live-fork` (a plain
VM fork) is shipped and is a different, simpler thing.
