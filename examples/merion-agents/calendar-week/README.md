# calendar-week

A calendar export (`.ics`) in, a list of what is on out: event titles (repeats counted), start times, places and
attendees. Repeated titles come first, which is a quick way to see what fills a week.

```bash
./scripts/merionctl deploy examples/merion-agents/calendar-week --test
```

It reads the export as text and does not understand time zones or recurrence rules: times are shown as exported
(`20250318T143000Z`), and a repeating event appears once. Folded lines (a long title continued on the next line) are
read up to the fold.

These are personal files. The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence class is `software-test`: whoever operates the host could still read a cell's memory. Do not promise more than that ([VENDORS.md](../../../docs/merion/VENDORS.md)).
