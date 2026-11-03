# outlook-calendar

`agenda` (the default) lists your next 24 hours (`days` up to 14), in UTC. `add` creates one event after a phone-signed approval that shows the title, time, place and guests.

**Microsoft emails every guest when an event is created**, so guests are only added when you say `notify: yes`; without it the agent refuses before sending anything. Times you give with an offset are sent as UTC. No edit or delete: `outlook-calendar-write` allows only creating an event. Not run against real Microsoft. Same chat form as [calendar-agent](../calendar-agent/README.md).
