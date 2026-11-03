# payslip-text

A payslip as text in, the pay in a page out: the month, earnings, deductions, net pay and every amount.

**Get the text:** open the payslip PDF and copy its text into a `.txt` file (or use your HR portal's text or CSV view). A scanned or photographed payslip has no text layer: use the photo packs instead. Payslip layouts vary a lot, so tune the keyword lists to the words yours uses; it reads what is written and does not check the arithmetic.

```bash
./scripts/merionctl deploy examples/merion-agents/payslip-text --test
./scripts/merionctl run payslip-text <file>
```

Has a synthetic sample, so `--test` works.
These are personal files. The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence class is `software-test`: whoever operates the host could still read a cell's memory. Do not promise more than that ([VENDORS.md](../../../docs/merion/VENDORS.md)).
