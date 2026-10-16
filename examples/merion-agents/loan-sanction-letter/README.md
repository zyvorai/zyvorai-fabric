# loan-sanction-letter

A sanction letter PDF in, the terms quoted back out: sanction terms (amount, rate, tenure, EMI, fees, moratorium),
conditions and covenants, charges and penalties, and every amount, rate and date the letter mentions.

```bash
./scripts/merionctl deploy examples/merion-agents/loan-sanction-letter --test
```

It quotes lines that contain the keywords; it does not compute EMIs, check the rate against a policy or judge the
terms. The PDF needs a text layer: a scanned letter has none and gives an empty brief (Keep does no OCR). Tune the
keyword lists to your own letter template. There is no sample, because a synthetic PDF adds little over the rules.

These are bank files. The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence class is `software-test`: whoever operates the host could still read a cell's memory. Keep makes no compliance claim; do not promise more than that ([VENDORS.md](../../../docs/merion/VENDORS.md), [BANK-OPERATIONS.md](../../../docs/merion/BANK-OPERATIONS.md)).
