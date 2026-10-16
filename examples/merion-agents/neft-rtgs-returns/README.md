# neft-rtgs-returns

A NEFT or RTGS returns / rejects report (`.txt` or `.csv`) in, a list out: the returned and rejected lines, the
beneficiary problems (account closed, does not exist, invalid IFSC, name mismatch, frozen, dormant), and every UTR,
IFSC and amount seen, counted.

```bash
./scripts/merionctl deploy examples/merion-agents/neft-rtgs-returns --test
```

The UTR pattern is four capitals, `R`/`N`/`H`, then 10–17 digits; the IFSC pattern is four capitals, `0`, six
characters. Check both against your own report before relying on them, and add your return-reason wording to the
keyword lists. Amounts are matched with `Rs`, `INR` or `₹` and the Indian digit grouping (`2,50,000.00`). It counts; it
does not add up or reconcile. The sample is synthetic (`EXMP`, `DEMO`, `SAMP` and `TEST` are not real bank codes).

These are bank files. The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence class is `software-test`: whoever operates the host could still read a cell's memory. Keep makes no compliance claim; do not promise more than that ([VENDORS.md](../../../docs/merion/VENDORS.md), [BANK-OPERATIONS.md](../../../docs/merion/BANK-OPERATIONS.md)).
