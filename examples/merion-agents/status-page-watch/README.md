# status-page-watch

A saved vendor status page in, a one-page summary out: what is down, degraded, under maintenance or recovered,
and which times it mentions. Declarative, so no code; the page is read as text and its scripts and styles are dropped.

```bash
./scripts/merionctl deploy examples/merion-agents/status-page-watch --test
```

**Scenario.** Have a CI job or cron fetch the vendor's status page and post it to a webhook trigger, or drop the
saved `.html` into a watched folder (see [TRIGGERS.md](../../../docs/merion/TRIGGERS.md)). Compare two days with
**Merion history → Runs → Compare selected**.

It matches keywords, so tune the lists to the words your vendors use. It reads the page as saved: nothing is fetched.
