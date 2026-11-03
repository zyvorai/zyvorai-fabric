# Model budgets

**Status:** built, off unless the operator's prices file sets them. Tested against a local stub provider.

## What exists

`ZYVOR_AGENT_MODEL_PRICES_FILE` already held a rolling 24-hour allowance per agent (`max_calls_per_day`,
`max_cost_per_day`), checked on both model paths (the use-case step and the `model_socket`).

## What this adds

An agent budget (or `default_budget`) can also carry:

```json
{"budgets": {"triage": {"max_calls_per_day": 500,
  "run":  {"max_calls": 40, "max_cost": 2.0},
  "goal": {"max_calls": 200}}}}
```

- **run**: one session, over its whole life.
- **goal**: every session started for the goal the calling session belongs to (its steps and its planning session).

Cost counts only calls that reported tokens on a priced model; the rest are not guessed. Counts come from the journal's
`model.call` rows, so a refused call is never counted. A call past a cap is refused with 429 and a message naming the
scope, before anything is sent, and the refusal is journaled as `model.budget_refused` with the reason. A prices file that cannot be read refuses rather than lifting the cap.

## Seeing what is left

`GET /v1/sessions/{id}/cockpit` (and `merionctl cockpit <id>`) carries a `budget` object: for the session's agent, one entry per
cap that is set (`day`, `run`, `goal`) with calls and cost used against the cap, and `reached` when the next call would be refused.
It reads the same journal rows the refusal reads, through one shared helper, so the two cannot disagree. It is `null` when no budget
applies, and `{"error": ...}` when the prices file cannot be read (calls are refused in that case too). The console shows it as a
"Model budget" card on the session page. It is read-only: nothing here changes a cap.

## Not done

- Per-user caps across agents.
- A warning before a cap is reached (only `reached` is reported, not a threshold).

- Caps in the manifest. They live in the operator's file so an agent's author cannot raise their own.
- Goal caps spanning agents other than the budgeted one only when those sessions belong to the same goal.
