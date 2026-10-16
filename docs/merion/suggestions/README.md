# Suggestions

Things an agent thinks you might want done, that **you** decide about. A suggestion does nothing by itself.

```
agent (scheduled run for you)  --suggestion.propose-->  host keeps it for you  -->  you: accept | dismiss
                                                                                      |
                                                            accept: an ordinary goal for you (no plan, not running)
```

## How it works

1. **You turn suggestions on** (off by default, per person): `PUT /v1/suggestions/settings {"enabled": true}` (the same route also takes `retention_days`, below). Until then every proposal is refused.
2. **An agent that asked for it** (`"suggestions": true` in its manifest) and runs for you emits `ctx.emit("suggestion.propose", {title, reason, agent?})`. `agent` names the deployed agent that would do it (default: the proposer). A typical source is a scheduled run (`POST /v1/schedules` with your `user_id`) that looks at something and works out an idea; the example [`suggestion-example`](../../../examples/merion-agents/suggestion-example/) shows just the mechanism.
3. **They wait for you**: `GET /v1/suggestions` (`pending`, and your recent `decided`) and in `GET /v1/inbox` (`suggestions`). An enrolled phone with a push relay also gets a **notice** (`suggestion.proposed`, generic text and only the suggestion's id; the title goes to a push vendor's relay only if the operator sets `ZYVOR_AGENT_PUSH_NOTICE_TEXT=1`). A notice can decide nothing.
4. **Accept** (`POST /v1/suggestions/{id}/accept`) makes an ordinary [goal](../goals/README.md) for you: the suggestion's title, the reason as its description, the agent named. It has **no plan and does not run**; you can [ask for a plan](../goals/README.md#plans-an-agent-proposes-and-you-accept) and accept that separately. Your goal limits apply. **Dismiss** (`POST /v1/suggestions/{id}/dismiss`) remembers it so the same suggestion is not made again.

## Limits and guards

- At most 20 waiting per person; a title of at most 120 and a reason of at most 500 plain-text characters (control, zero-width and direction-changing characters removed); text that looks like a credential is refused.
- A suggestion whose title (ignoring case and spacing) matches one already waiting, accepted or dismissed is refused, so an agent cannot nag. The last 200 decided ones are remembered per person.
- A session that had read untrusted content marks its suggestion `tainted`; accepting it needs `{"confirm_tainted": true}`.
- A refused proposal shows up in the session's events as `suggestion.refused` with the reason, never the text.
- Every route is scoped to the caller: another person's suggestion is a 404. The operator must name `user_id` and every access is journaled (`keep.suggestion.operator_access`). The journal records that a suggestion was proposed, accepted or dismissed (ids), never its text.
- **Retention** is per person: `PUT /v1/suggestions/settings {"retention_days": N}` (1 to 3650, returned back in `GET /v1/suggestions`) forgets undecided suggestions that old and decided ones that long after the decision (a forgotten dismissal can be proposed again). The operator's `ZYVOR_AGENT_SUGGESTION_RETENTION_DAYS` is only the default for a person who hasn't set their own. The last 200 decided per person are kept regardless. Counts only in the journal.
- Like the vault and memory, the operator of the host can read the files (`suggestions/<user>.json`); they are not encrypted to you.

An empty request body with `content-type: application/json` means "no options" (for accept, and for `POST /v1/goals/{id}/plan` and its accept).

## Verified, and what is not

- `tenancy_tests::suggestions_are_opt_in_private_bounded_and_only_the_owner_turns_them_into_goals` (two users and an operator against the real router: off by default, a repeat, an agent that did not ask, no user, a secret; privacy and 404s; the inbox; the journal holds no text; accept makes a plan-less goal; tainted needs confirmation; dismissed is not re-proposed; the 20 cap; operator access) and the route table test in `authz`. Each guard was checked by mutation.
- `demos-ci.sh`: the real runtime with the example agent in a stub cell: refused while off; waits for one person only; accept makes a plan-less, not-running goal; the same suggestion is not made again.
- **Not built or verified:** a real scheduled run over time in production was not watched. The chat page, the iPhone app and Solvor's Goals pane all show them now; the example [`suggestion-example`](../../../examples/merion-agents/suggestion-example/) only relays its input, but [`calendar-suggestions`](../../../examples/merion-agents/calendar-suggestions/) is a real heuristic finder (no model call) over calendar-like state — an event tomorrow with no note, or no place, becomes a suggestion by plain rules.
