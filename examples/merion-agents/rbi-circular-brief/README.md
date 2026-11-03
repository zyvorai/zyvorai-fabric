# rbi-circular-brief

A regulator circular PDF in, a brief out: its `RBI/YYYY-YY/NN` references, who it applies to, effective dates and
deadlines, the lines that say what must be done (`shall`, `required to`, `must`), and what it repeals or amends.

```bash
./scripts/merionctl deploy examples/merion-agents/rbi-circular-brief --test
```

It is a reading aid for the compliance team, not an interpretation: it quotes lines, it does not decide whether a
circular applies to your bank. Dates are matched in the `April 1, 2025` style; add a pattern if your circulars use
another. Scanned circulars have no text layer and give an empty brief (no OCR).

These are bank files. The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence class is `software-test`: whoever operates the host could still read a cell's memory. Merion makes no compliance claim; do not promise more than that ([VENDORS.md](../../../docs/merion/VENDORS.md), [BANK-OPERATIONS.md](../../../docs/merion/BANK-OPERATIONS.md)).
