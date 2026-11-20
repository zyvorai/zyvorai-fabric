# goal-planner

Proposes the steps of a [goal](../../../docs/merion/goals/README.md). Set `ZYVOR_AGENT_PLANNER_AGENT=goal-planner` on the host (or name it in the request), then:

```bash
curl -X POST -H "Authorization: Bearer $TOKEN" $KEEP/v1/goals -d '{"title":"Plan a weekend in Lisbon","agent":"mail-compose"}'   # no plan yet
curl -X POST -H "Authorization: Bearer $TOKEN" $KEEP/v1/goals/$ID/plan                 # the planner runs and proposes steps
curl -H "Authorization: Bearer $TOKEN" $KEEP/v1/goals/$ID                               # see proposed_plan
curl -X POST -H "Authorization: Bearer $TOKEN" $KEEP/v1/goals/$ID/plan/accept           # or /plan/reject
```

It uses the pack's `model_socket` (edit `base_url`, `model` and `credential` to your endpoint; the credential must be in the host vault, and the first use needs an approval like any model call). Its output is a proposal only: nothing runs until you accept, each step is then an ordinary session with every policy and approval of one, and running them automatically is the goal's own opt-in (`autorun`).

The model is asked for one step per line; `parseSteps` strips list markers and keeps at most `max_steps`. Each step's input is `{"message": "<the line>"}`, which is what chat-style agents take.
