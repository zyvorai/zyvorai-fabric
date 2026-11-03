# reimbursement-claims

A mail export in, a claims list out: who is asking, every amount, the lines that say a claim is approved or paid, the ones
pending or declined, and the categories (cab, hotel, meals and so on).

```bash
./scripts/merionctl deploy examples/merion-agents/reimbursement-claims --test
```

It matches words and currency markers, so add your company's wording to `pack.json`. It does not add the amounts, match a
claim to a receipt or decide eligibility, and it never sends the reply; drafting and sending stay with you.

**Not this:** these packs summarise a file you export or save (a mail export, a PO as text, a CSV). They are not your books of account, do not validate a GSTIN or work out tax, do not post to an accounting or payroll system, and do not send mail; the official invoice, ledger or return stays in your own systems. They add and compare nothing: they list and count.

These files hold business and personal data (amounts, tax ids, salaries). The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence class is `software-test`: whoever operates the host could still read a cell's memory. Merion bank details, Aadhaar and full ids out of the exports ([VENDORS.md](../../../docs/merion/VENDORS.md)).
