# android-call-log

A call-log CSV in, a summary out: calls by type (incoming, outgoing, missed, rejected), the people you talk to most, the numbers, and the first rows.

**Get the file:** export the call log with a backup app that writes CSV (for example a call-log backup app for Android), or from your carrier's portal. The pack reads columns named `type`, `name` and `number` (case does not matter); rename the header row if yours differs. Call logs are personal: the cell has no network, but see the caveat below.

```bash
./scripts/merionctl deploy examples/merion-agents/android-call-log --test
./scripts/merionctl run android-call-log <file>
```

Has a synthetic sample, so `--test` works.
These are personal files. The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence class is `software-test`: whoever operates the host could still read a cell's memory. Do not promise more than that ([VENDORS.md](../../../docs/merion/VENDORS.md)).
