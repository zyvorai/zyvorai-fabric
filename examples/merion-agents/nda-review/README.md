# nda-review

Reads a Word `.docx` and lists the clauses you usually check first, plus the durations and amounts it mentions.
It **finds passages by keyword**; it does not understand the contract or judge it. Treat it as a checklist that
saves you scrolling, not as legal advice.

```bash
./scripts/merionctl deploy examples/merion-agents/nda-review
./scripts/merionctl run nda-review vendor-nda.docx
```

No sample is bundled (a `.docx` is not text). The file never leaves the sealed cell: 0 CONNECT.
For a generated plain-language summary on top, see [invoice-model-brief](../invoice-model-brief/README.md),
which shows how a model step is declared and gated.
