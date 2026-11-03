# sales-register-sheet

An Excel sales register in, a view out: how many rows each customer has and the first rows as a table.

Expected columns (first sheet, row 1): `date`, `customer`, `invoice_no`, `taxable_value`, `tax`, `total`. Rename them in `pack.json` for your sheet.

```bash
./scripts/merionctl deploy examples/merion-agents/sales-register-sheet
./scripts/merionctl run sales-register-sheet register.xlsx
```

Only the first sheet is read (up to 5000 rows). **It adds nothing up**: no totals, no tax check; that stays in your accounting tool. There is no bundled sample (a sample must be text); CI builds a small workbook. Use it with made-up figures first.

**Not this:** Merion does not connect to your accounts or tools. It reads a file you export or save, in a sealed Linux cell, and lists and counts what is in it. It is not a scanner or a replacement for the tool that produced the file.

The output can name people, repositories, apps and paths. The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence class is `software-test`: whoever operates the host could still read a cell's memory ([VENDORS.md](../../../docs/merion/VENDORS.md)). Merion secrets, tokens and private keys out of the files.
