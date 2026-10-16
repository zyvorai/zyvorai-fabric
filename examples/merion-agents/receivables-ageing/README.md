# receivables-ageing

A mail export in, a receivables picture out: the invoice numbers mentioned (the ones chased most often first; a number in both the subject and the body counts twice), the overdue and
unpaid lines, payments received, due dates, amounts and who is writing.

```bash
./scripts/merionctl deploy examples/merion-agents/receivables-ageing --test
```

It expects invoice numbers shaped `INV-...` or `INV/...`; change the first pattern in `pack.json` for your numbering. It does
not work out ageing buckets (0-30, 31-60 and so on): it has no date arithmetic, so it lists the due dates and dates it finds.
Amounts recognise `₹`, `Rs`, `INR`, `USD`, `EUR`, `GBP`, `$`, `€` and `£`, including the `1,25,000` grouping.

**Not this:** these packs summarise a file you export or save (a mail export, a PO as text, a CSV). They are not your books of account, do not validate a GSTIN or work out tax, do not post to an accounting or payroll system, and do not send mail; the official invoice, ledger or return stays in your own systems. They add and compare nothing: they list and count.

These files hold business and personal data (amounts, tax ids, salaries). The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence class is `software-test`: whoever operates the host could still read a cell's memory. Keep bank details, Aadhaar and full ids out of the exports ([VENDORS.md](../../../docs/merion/VENDORS.md)).
