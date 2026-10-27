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
scope, before anything is sent. A prices file that cannot be read refuses rather than lifting the cap.

## Not done

- A dedicated journal row for the refusal (the 429 and its message are the record today).
- Caps in the manifest. They live in the operator's file so an agent's author cannot raise their own.
- Goal caps spanning agents other than the budgeted one only when those sessions belong to the same goal.
