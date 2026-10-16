# upi-dispute-mail

Customer dispute mail (`.eml` or `.mbox`) in, a list out: what customers report (debited, not received, failed,
pending, reversal, refund, chargeback), the 12-digit reference numbers they quote (repeats counted, so follow-ups
show up), every amount, and the messages that mention a complaint, the ombudsman or a legal notice.

```bash
./scripts/merionctl deploy examples/merion-agents/upi-dispute-mail --test
```

The reference pattern is any 12-digit number, which is what a UPI RRN looks like, but a 12-digit account number would
match too: mask account numbers upstream or tighten the pattern. It lists; it does not decide a dispute or reply to
anyone. The sample is synthetic.

These are bank files. The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence class is `software-test`: whoever operates the host could still read a cell's memory. Keep makes no compliance claim; do not promise more than that ([VENDORS.md](../../../docs/merion/VENDORS.md), [BANK-OPERATIONS.md](../../../docs/merion/BANK-OPERATIONS.md)).
