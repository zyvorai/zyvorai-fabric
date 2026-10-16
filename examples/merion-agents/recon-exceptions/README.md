# recon-exceptions

A reconciliation exceptions export (`.csv`) in, exceptions counted by `exception_type`, `channel` and
`ageing_bucket`, plus the first rows and the file size. A quick way to see, each morning, which channel and which age
band the open items sit in.

```bash
./scripts/merionctl deploy examples/merion-agents/recon-exceptions --test
```

Rename the columns in `pack.json` to match your reconciliation tool's export. It counts; it does not match entries or
total amounts. Run it daily and compare two runs in **Merion history** to see what moved. The sample is synthetic.

These are bank files. The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence class is `software-test`: whoever operates the host could still read a cell's memory. Merion makes no compliance claim; do not promise more than that ([VENDORS.md](../../../docs/merion/VENDORS.md), [BANK-OPERATIONS.md](../../../docs/merion/BANK-OPERATIONS.md)).
