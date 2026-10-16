# deck-outline

A PowerPoint file in, an outline out: how many slides, every amount (with the currency before or after the number) and date on them, open points (`TBD`, `draft`, `confidential`), action and owner lines, and the speaker notes.

```bash
./scripts/merionctl deploy examples/merion-agents/deck-outline
./scripts/merionctl run deck-outline board-update.pptx
```

Reads the `.pptx` slide text in presentation order (a slide is `## Slide N`) and each slide's speaker notes (`Notes: ...`). It does not read images, charts, embedded tables inside pictures, or the legacy binary `.ppt`. Slide-number fields are dropped. There is no bundled sample (a sample must be text); CI builds a small deck.

**Not this:** it lists and counts what is on the slides; it does not judge the content, check figures against a source, or edit the deck.

The deck can hold business data, and the evidence class is `software-test`: whoever operates the host could still read a cell's memory ([VENDORS.md](../../../docs/merion/VENDORS.md)).
