# Keep goals, plans, and artifacts

Thin product loop on agent-runtime (not a separate platform):

| Object | API | Notes |
|---|---|---|
| Goal | `POST/GET /v1/goals`, `GET/PATCH /v1/goals/{id}` | State-dir JSON (`goals.json`) |
| Plan step | embedded on goal; `POST /v1/goals/{id}/advance` | `pending` / `running` / `blocked` / `done` / `skipped` |
| Artifact | `POST/GET /v1/artifacts`, `GET /v1/artifacts/{id}` | Reports; soft-refuses secret-shaped bodies |
| Approval | reuse `/v1/approvals` | Advance to done with `requires_approval` + `session_id` opens a pending approval and blocks the step. Asking again returns 409 while it is pending; a denied or expired approval keeps the step blocked; only an approved one lets the step finish. A cancelled goal cannot be advanced |

Sessions still execute work. Goals orchestrate status and evidence, and a goal that opts in can also **run its own plan** (next section). Cockpit
(`GET /v1/sessions/{id}/cockpit`) includes `active_goal` and `recent_artifacts`
hrefs.

## The goal worker (opt-in)

Set `"autorun": true` when creating a goal (or `PATCH` it later) and a background worker runs the plan **one step at a time**, so nobody has to call `advance`.
A goal that does not say `autorun` is never touched by the worker, and `PATCH {"autorun": false}` pauses one at once (no further step starts).

```bash
curl -X POST "$MERION_API/v1/goals" -H "Authorization: Bearer $OPERATOR_TOKEN" -H 'content-type: application/json' -d '{
  "title": "Weekly report", "agent": "report-agent", "user_id": "ana", "autorun": true, "max_attempts": 3,
  "plan": [
    {"title": "Collect", "input": {"folder": "inbox"}},
    {"title": "Draft the summary", "requires_approval": true},
    {"title": "File it", "input": {"to": "archive"}}
  ]
}'
```

For the first step that is not done or skipped the worker does exactly one thing per look (every `ZYVOR_AGENT_GOAL_TICK_MS`, default 5000 ms):

| The step is | The worker |
|---|---|
| pending | starts a session of the goal's agent with the step's `input` (or the goal and step titles: `message`, `goal`, `step`). The session request id is `goal:<goal>:<step>:<attempt>`, so a restart while starting one finds the session it already made. Runs as the goal's `user_id`, with that user's run quota; a quota refusal is not an attempt, the worker just looks again later. |
| running, its session completed | marks it done, or, if the step has `requires_approval`, opens an approval bound to the step's session (`kind: send`) and blocks the step (`blocked_on: approval`) — if the step carries an `approval_reason`, it appears both in the approval's prompt ("...The agent said: {reason}") and structured on the approval's `planned_action`. |
| running, its session failed, was cancelled or expired | tries again after a delay that doubles (`ZYVOR_AGENT_GOAL_RETRY_BASE_SECS`, default 15 s, at most 10 min), up to `max_attempts` (default 3, 1 to 10); then blocks the step and the goal with `failed after N attempts: <error>` (`blocked_on: failure`). |
| waiting for approval | approved: done. Denied or expired: stays blocked (`blocked_on: rejected`) and **the worker never goes past a refusal**. |
| blocked for any other reason | nothing: a blocked step is never retried by the worker. |

An approval step here is a **checkpoint after the step ran**, before the goal moves on (an approval needs a session, and that is how the user's ownership of it is decided). It does **not** gate the step's own actions: a real send or purchase is decided where it happens, at the egress broker (a credential with `requires_approval`), which every step's session goes through like any other.

What the worker never does: widen any authority (a step's session is an ordinary session with every policy, quota and approval of one started by hand), approve anything itself, run two steps of a goal at once, or retry what it blocked. Cancelling the goal (`PATCH {"status":"cancelled"}`) cancels the running step's session. A goal with `autorun` refuses `advance` (409) so two hands never move one goal; pause it first if you want to.
Each start, retry, block and completion is journaled (`merion.goal.step.started`, `.retry`, `.blocked`, `.done`, `.approval`, `.cancelled`) with ids and reasons, never step inputs.

## Goals for a user

A user token can manage its **own** goals: `GET /v1/goals` (own only), `POST /v1/goals` (always for itself; a `user_id` naming someone else is a 400), `GET /v1/goals/{id}` and `PATCH /v1/goals/{id}`. Another user's goal is a 404 for every probe. What a user can do is deliberately narrower than the operator:

| | User token | Operator |
|---|---|---|
| create | own goals only, for a deployed agent, at most 20 steps, at most 100 goals, at most 5 running automatically at once | any |
| `session_id`, `allow_hosts`, replacing the plan | refused (403) | yes |
| `PATCH` | `status: "cancelled"`, `autorun` on or off (within the cap; a finished or cancelled goal cannot be switched on again) | anything |
| `advance` by hand, `browse` under a goal | closed | yes |

A user's goal runs as that user: its step sessions carry their `user_id`, use their run quota and their memory, and any approval it opens is theirs to decide on their device (`/v1/approvals`).

**In the chat page.** The Goals tab of `scripts/merion-chat.py` creates, lists, pauses, resumes and cancels a person's goals for the agent it is fixed to (see [AGUI.md](../AGUI.md)). Verified in a real browser against a real local runtime and the simulator: a two-step goal created from the page ran by itself, and its second step (`memory-agent`: "remember I like aisle seats") produced a suggestion that appeared under Memory, was accepted there, and was then used by the agent in a chat. Not clicked through: pausing or cancelling from the page (the proxy routes are unit tested), the phone layout of these tabs, and light mode.

**Plans and suggestions in the chat page.** A goal added with no steps gets an "Ask the agent for a plan" button (`POST /v1/goals/{id}/plan`; the page cannot name the planner, the host's `ZYVOR_AGENT_PLANNER_AGENT` decides, and the host's "no planner agent" answer is shown). A proposed plan is listed with its step titles and whether a step asks you first, with Accept, Accept and run, and Discard; a plan from a planner that had read untrusted content needs a second click on Accept. Above the goals, suggestions can be turned on and the [suggestions](../suggestions/README.md) waiting for you are listed with "Make it a goal" and Dismiss (a tainted one needs a second click). The proxy forwards only the two switches `confirm_tainted` and `autorun` (only when literally `true`), lets only ids the host itself lists for this person and agent through, and requires the same origin for every write; 5 new proxy tests cover that. Driven in a real browser against a fake host: the suggestions, the proposed plan and its two-click accept sent `{"confirm_tainted":true,"autorun":false}`, and dismiss sent nothing. **Not verified:** this page against a real runtime with a real planner (the runtime side is covered by `demos-ci.sh`), and the phone layout.

**Known limits.** The worker looks at goals on a timer (a step starts up to one tick after the last one ends). The plan is what you give it, or one an agent proposes and you accept ([Plans an agent proposes](#plans-an-agent-proposes-and-you-accept)); how good a real model's plan is has not been tried. Updates race with a concurrent `PATCH` in a small window: the worker re-reads the goal before saving and drops its move if the step or the cancellation changed, and the next tick looks again.
**Tests.** `tenancy_tests::goals_are_private_bounded_and_only_cancelled_or_paused_by_their_owner` (two users and the operator against the real router: forced owner, refused fields, the caps, cross-user 404s, cancel and pause; mutation checks: dropping the list filter or loosening the cap fail it), `demos-ci.sh` (a user's automatic goal is created with their token, runs as them and finishes, another user cannot see or cancel it), and `goal_worker::tests` (9: every move of the state machine, with three mutation checks: a refusal counted as approval, retries ignoring `max_attempts`, and a blocked failure being restarted each fail a test), `goals::tests` (creation limits, no hand-advancing an autorun goal), and `demos-ci.sh` against a real runtime and the stub cell: a two-step plan runs in order with each step's own input and refuses a hand on it; a failing agent is retried and then blocks the goal with the reason while the next step never starts and nothing retries after the block; an approval step waits, approval finishes the goal, denial stops it for good; a goal without `autorun` is left alone, switching it on runs it, pausing stops further steps.

Packaged agents: [`examples/merion-agents/`](../../../examples/merion-agents/).
Demo: [`./scripts/merion-pack-demo.sh`](../../../scripts/merion-pack-demo.sh).

See also [PRODUCTION.md](../PRODUCTION.md) and [Tutorial 16](../../tutorials/16-merion-workstation.md).

## Plans an agent proposes, and you accept

A goal can start with no plan (`POST /v1/goals` with just a `title`, a `description` and the `agent` that will do the work). Then:

1. `POST /v1/goals/{id}/plan` (optionally `{"planner": "<agent>"}`; otherwise the host's `ZYVOR_AGENT_PLANNER_AGENT`) runs a **planner agent** as an ordinary session for the goal's user, given the goal's title and description. `202` with the session id. The example [`goal-planner`](../../../examples/merion-agents/goal-planner/) asks its model socket for one step per line.
2. The planner answers by emitting a `goal.plan_proposed` event with `{steps: [{title, input?, requires_approval?, approval_reason?}]}`.
   The host stores it on the goal as `proposed_plan`. **The plan stays empty and nothing runs.** `approval_reason` is the planner's own
   words for why that step needs a person (at most 240 plain-text characters, and only accepted alongside `requires_approval: true`); if
   the step later opens an approval, the reason appears both in the approval's prompt and structured on its `planned_action`.
3. You read `GET /v1/goals/{id}` and either `POST /v1/goals/{id}/plan/accept` (the steps become the plan, ids `s1`, `s2`, ...; optionally `{"autorun": true}` to let the [worker](#the-goal-worker-opt-in) run them, which is still the goal's own opt-in) or `POST /v1/goals/{id}/plan/reject`.

What keeps a model's plan from being a back door: only the planning session started for that goal can propose, and only once; a proposal is bounded (1 to 10 steps, titles of at most 120 plain-text characters with control, zero-width and direction-changing characters removed, a step input that is an object of at most 4 KiB); a planner that had read untrusted content is marked `tainted` and its plan needs `{"confirm_tainted": true}` to accept; a goal that already has a plan is not planned again and a late proposal never replaces it; an earlier proposal stays until the new one arrives or you decide; an `approval_reason` is bounded the same way as a title (240 plain-text characters, control/zero-width/direction-changing characters removed). Accepting changes nothing else: each step still runs as an ordinary session with every policy, quota and approval of one, and a step that sends or spends still waits for your phone. The audit journal records that a plan was requested, proposed (with the number of steps) and accepted or rejected, never the steps.

A user token can do all three for its own goals (another user's goal is a 404); the planner runs with the user's identity, so the user's run quota applies.

