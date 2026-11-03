# fuel-receipt-photo

A photo of a fuel receipt in, the numbers out: litres, rate, total and date. Read by OCR (English, `tesseract` in the cell), so check the amounts against the receipt; a crumpled or faded thermal receipt may read badly.

```bash
./scripts/merionctl deploy examples/merion-agents/fuel-receipt-photo
./scripts/merionctl run fuel-receipt-photo <file>
```

There is no bundled sample (a photo cannot be one), so `--test` is not available.

These are personal files. The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence class is `software-test`: whoever operates the host could still read a cell's memory. Do not promise more than that ([VENDORS.md](../../../docs/merion/VENDORS.md)).
