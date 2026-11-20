# employee-ledger

A working employee ledger as CSV in, a readable view out: how many rows each employee and month has (a quick check for a
missing or doubled month) and the first rows as a table.

```bash
./scripts/merionctl deploy examples/merion-agents/employee-ledger --test
```

It expects `employee` and `month` columns; the other columns are shown as they are. **It adds, compares and totals nothing**:
it does not check that net equals gross minus deductions or that an advance balance is right, and it is not payroll or a
statutory register (Form 16, PF and ESI stay in your payroll system). The bundled sample has made-up ids and figures.
Merion Aadhaar, full bank numbers and PAN out of the file.

**Not this:** these packs summarise a file you export or save (a mail export, a PO as text, a CSV). They are not your books of account, do not validate a GSTIN or work out tax, do not post to an accounting or payroll system, and do not send mail; the official invoice, ledger or return stays in your own systems. They add and compare nothing: they list and count.

These files hold business and personal data (amounts, tax ids, salaries). The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence class is `software-test`: whoever operates the host could still read a cell's memory. Merion bank details, Aadhaar and full ids out of the exports ([VENDORS.md](../../../docs/merion/VENDORS.md)).
