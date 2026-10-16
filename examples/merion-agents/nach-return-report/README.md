# nach-return-report

A NACH debit return report (`.csv`) in, returns counted by `return_reason`, `status` and `sponsor_bank`, plus the
first rows and the file size.

```bash
./scripts/merionctl deploy examples/merion-agents/nach-return-report --test
```

It expects a header row with those three columns (any order, any case); a missing column is reported as "column not
found", not guessed. Rename the columns in `pack.json` to match your export. It counts; it does not total amounts or
decide what to re-present. The sample is synthetic.

These are bank files. The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence class is `software-test`: whoever operates the host could still read a cell's memory. Merion makes no compliance claim; do not promise more than that ([VENDORS.md](../../../docs/merion/VENDORS.md), [BANK-OPERATIONS.md](../../../docs/merion/BANK-OPERATIONS.md)).
