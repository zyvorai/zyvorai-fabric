# school-fee-receipt-photo

A photo of a school fee receipt in, the facts out: receipt number, student and term, what was paid and any balance due. Read by OCR (English), so check the amounts against the receipt; stamps and handwriting are often missed.

```bash
./scripts/merionctl deploy examples/merion-agents/school-fee-receipt-photo
./scripts/merionctl run school-fee-receipt-photo <file>
```

There is no bundled sample (a photo cannot be one), so `--test` is not available.

These are personal files. The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence class is `software-test`: whoever operates the host could still read a cell's memory. Do not promise more than that ([VENDORS.md](../../../docs/merion/VENDORS.md)).
