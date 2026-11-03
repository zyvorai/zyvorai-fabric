# contacts-audit

A contacts export (`.vcf`) in, a tidy-up list out: how many cards, each name with a count (a name that appears more
than once is a likely duplicate and is listed first), phone numbers and emails.

```bash
./scripts/merionctl deploy examples/merion-agents/contacts-audit --test
```

It lists what is in the file; it does not merge, delete or change anything, and it does not match a name against a
number. Photos and other vCard fields are ignored. The output contains the numbers and addresses, so treat the
summary as sensitive as the export.

These are personal files. The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence class is `software-test`: whoever operates the host could still read a cell's memory. Do not promise more than that ([VENDORS.md](../../../docs/merion/VENDORS.md)).
