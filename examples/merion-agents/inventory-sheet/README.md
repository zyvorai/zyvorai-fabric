# inventory-sheet

An Excel stock sheet in, a view out: rows per location and the first rows as a table.

Expected columns: `sku`, `description`, `qty`, `location`, `reorder_level`. Rename them in `pack.json` for your sheet.

```bash
./scripts/merionctl deploy examples/merion-agents/inventory-sheet
./scripts/merionctl run inventory-sheet stock.xlsx
```

First sheet only, up to 5000 rows. **It does not compare quantity with the reorder level** (no arithmetic); it shows the rows. No bundled sample; CI builds a small workbook.

**Not this:** Keep does not connect to your accounts or tools. It reads a file you export or save, in a sealed Linux cell, and lists and counts what is in it. It is not a scanner or a replacement for the tool that produced the file.

The output can name people, repositories, apps and paths. The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence class is `software-test`: whoever operates the host could still read a cell's memory ([VENDORS.md](../../../docs/merion/VENDORS.md)). Keep secrets, tokens and private keys out of the files.
