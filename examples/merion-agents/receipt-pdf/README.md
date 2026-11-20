# receipt-pdf

A receipt or warranty PDF in, the money and the terms out: totals, dates, warranty and return lines, the items, and every
amount seen.

```bash
./scripts/merionctl deploy examples/merion-agents/receipt-pdf
./scripts/merionctl run receipt-pdf receipt.pdf
```

Needs the PDF reader (`poppler`) in the cell template, like the built-in `pdf-brief`. A **scanned PDF has no text layer and is refused**; for a photo or screenshot of a receipt use
[`receipt-photo`](../receipt-photo/README.md), which reads the picture with OCR. There is no bundled sample, so `--test` is not available.

These are personal files. The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence class is `software-test`: whoever operates the host could still read a cell's memory. Do not promise more than that ([VENDORS.md](../../../docs/merion/VENDORS.md)).
