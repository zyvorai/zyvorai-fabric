# subscription-finder

A mail export in, "what am I paying for" out: renewal notices, trials that are about to end, the lines that say you
will be charged, the amounts, and who is charging.

```bash
./scripts/merionctl deploy examples/merion-agents/subscription-finder --test
```

**Scenario.** A user shares a month of billing mail (an `.mbox` export) from the vendor app. Compare two months with
**Keep history → Runs → Compare selected** to see what is new.

Keywords are English; add your language's words in `pack.json`. It reads text bodies only. For a general mailbox
summary see [mailbox-triage](../mailbox-triage/README.md).

These are personal files. The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence class is `software-test`: whoever operates the host could still read a cell's memory. Do not promise more than that ([VENDORS.md](../../../docs/merion/VENDORS.md)).
