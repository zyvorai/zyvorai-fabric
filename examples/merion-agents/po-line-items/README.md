# po-line-items

A purchase order as text in, the facts to copy into an invoicing app out: the PO number, GSTINs, HSN or SAC codes, the
line-item and total lines, open points (`TBD`, `to be confirmed`) and every amount.

```bash
./scripts/merionctl deploy examples/merion-agents/po-line-items --test
```

It reads text. For a PDF PO, run `pdftotext po.pdf po.txt` first, or copy the pack and set `"extract": "pdftotext"`,
`"accepts": ["pdf"]`. It lists GSTINs by their format only; it does not check them against any portal, decide intra- or
inter-state tax, or say whether an HSN code is missing (it lists the ones present, so a missing one is the gap you see).
The sample uses made-up identifiers.

**Not this:** these packs summarise a file you export or save (a mail export, a PO as text, a CSV). They are not your books of account, do not validate a GSTIN or work out tax, do not post to an accounting or payroll system, and do not send mail; the official invoice, ledger or return stays in your own systems. They add and compare nothing: they list and count.

These files hold business and personal data (amounts, tax ids, salaries). The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence class is `software-test`: whoever operates the host could still read a cell's memory. Merion bank details, Aadhaar and full ids out of the exports ([VENDORS.md](../../../docs/merion/VENDORS.md)).
