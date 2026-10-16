# travel-itinerary

A booking or boarding-pass email (`.eml`, or an `.mbox` of several) in, the trip out: flight and check-in lines, hotel
lines, booking references and amounts. A reference that appears in the subject and the body is counted twice.

```bash
./scripts/merionctl deploy examples/merion-agents/travel-itinerary --test
```

It matches words such as `departs`, `gate` and `hotel`, so a confirmation in another language needs its own
keywords. It does not read PDF or image boarding passes (no OCR); forward the email itself.

These are personal files. The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence class is `software-test`: whoever operates the host could still read a cell's memory. Do not promise more than that ([VENDORS.md](../../../docs/merion/VENDORS.md)).
