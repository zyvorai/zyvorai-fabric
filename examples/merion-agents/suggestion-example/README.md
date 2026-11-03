# suggestion-example

The smallest agent that makes [suggestions](../../../docs/merion/suggestions/README.md): it emits a `suggestion.propose` event for each `{title, reason, agent?}` in its input (at most five). `"suggestions": true` in `pack.json` is what allows it, and each person must also turn suggestions on. A proposal only waits for the person; accepting one makes an ordinary goal.

Run it on a schedule for one person (`POST /v1/schedules` with `user_id`) once a real agent works the ideas out.
