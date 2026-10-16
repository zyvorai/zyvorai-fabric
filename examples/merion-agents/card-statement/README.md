# card-statement

A statement CSV in, the most common categories and merchants and the first rows out. It expects a header row with
`category` and `description` columns (any order, any case); a missing column is reported as "column not found", not
guessed. Rename the columns in `pack.json` to match your bank's export.

```bash
./scripts/merionctl deploy examples/merion-agents/card-statement --test
```

It counts; it does not add up amounts or judge them. For a spreadsheet instead of a CSV, use
[expense-sheet](../expense-sheet/README.md).

These are personal files. The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence class is `software-test`: whoever operates the host could still read a cell's memory. Do not promise more than that ([VENDORS.md](../../../docs/merion/VENDORS.md)).
