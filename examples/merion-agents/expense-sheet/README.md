# expense-sheet

Reads the **first sheet** of an `.xlsx` as CSV (up to 5000 rows). The first row must be headers; the rules look for
columns named `category` and `vendor` (any case). Change them in `pack.json` to match your sheet.

```bash
./scripts/merionctl deploy examples/merion-agents/expense-sheet
./scripts/merionctl run expense-sheet march-expenses.xlsx jan.xlsx feb.xlsx   # one cell per file
```

There is no bundled sample, because a spreadsheet is not text. The demos e2e builds a small one.

Formulas show their last saved value. Cells are shown as text, with markup removed. Nothing in the file is run.
