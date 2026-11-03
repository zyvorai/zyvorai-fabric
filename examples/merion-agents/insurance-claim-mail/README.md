# insurance-claim-mail

Insurer emails in, a claims status page out: claim numbers, status (approved, pending, under review, rejected), amounts, dates, and what they still need from you.

**Get the file:** export the messages as `.eml`, or the folder as `.mbox`. It matches the words insurers commonly use (see the keyword lists) and does not decide anything about your claim; read the original before you act.

```bash
./scripts/merionctl deploy examples/merion-agents/insurance-claim-mail --test
./scripts/merionctl run insurance-claim-mail <file>
```

Has a synthetic sample, so `--test` works.
These are personal files. The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence class is `software-test`: whoever operates the host could still read a cell's memory. Do not promise more than that ([VENDORS.md](../../../docs/merion/VENDORS.md)).
