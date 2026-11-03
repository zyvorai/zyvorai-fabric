# calendar-suggestions

The "real agent" [suggestion-example](../suggestion-example/README.md) says to run once it exists: reads the next day of the calendar (`calendar-read`, read-only) and proposes, by plain rules — no model, nothing learned — a suggestion for an event that could use a little more preparation:

- an event with no notes attached at all gets a "prepare for it" suggestion;
- an event with other people invited but no location and no video link gets an "add a place" suggestion.

Each rule (`needsPrepReminder`, `needsAPlace`) looks at one event in isolation; `findSuggestions` is the whole decision and is unit-tested without a network (`sdk/agent-runtime/test/calendar-suggestions.test.js`). Like every [suggestion](../../../docs/merion/suggestions/README.md), a proposal only waits: the person decides, and accepting one makes an ordinary goal, nothing more.

Meant to run on a schedule, once a day, for one person:

```
POST /v1/schedules   (operator)
{"agent": "calendar-suggestions", "cron": "0 7 * * *", "user_id": "ana", "input": {"days": 1}}
```

Tested against the fake Google stub used elsewhere in this repo, not yet run against a real calendar.
