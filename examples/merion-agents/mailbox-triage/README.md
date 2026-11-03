# mailbox-triage

Reads an `.mbox` or `.eml` export directly (headers and text bodies, HTML-only messages reduced to text; up to
500 messages) and lists who wrote, the subjects, and lines about replies, money and meetings. No model reads your mail.

```bash
./scripts/merionctl deploy examples/merion-agents/mailbox-triage --test
```

**Scenario.** Export a month of mail and drop it in, or point a watched folder at your export directory. Compare
with [inbox-digest](../inbox-digest/README.md), which takes a plain-text export instead.

Attachments are ignored. Base64 and quoted-printable bodies are decoded; other encodings arrive as raw text.
