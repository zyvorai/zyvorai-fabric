# bank-sms-ledger

Saved bank and card text messages in, a ledger out: money out, money in, every amount seen, where the money went, and
the lines that need a look (declined, failed, international, over the limit).

```bash
./scripts/merionctl deploy examples/merion-agents/bank-sms-ledger --test
```

**One-time passwords are never listed.** No rule matches OTP wording and the amount pattern needs a currency
marker, so a code such as `482913` does not appear in the summary. Check the output of your own bank's wording
before relying on that: a message that puts a code next to a currency word could match.

It matches keywords and currency markers (`Rs`, `INR`, `USD`, `EUR`, `GBP`, `$`, `€`, `£`), so tune both lists to
your bank. The merchant pattern expects merchants in capitals (`at STREAMCO`).

These are personal files. The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence class is `software-test`: whoever operates the host could still read a cell's memory. Do not promise more than that ([VENDORS.md](../../../docs/merion/VENDORS.md)).
