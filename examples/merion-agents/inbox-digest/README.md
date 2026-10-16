# inbox-digest (a use case you define, not a built-in)

Drop in an exported mailbox and get a digest: what needs a reply, money, meetings, bulk mail, and who
writes most. A declarative use case, so there is no code, only an extractor and a few rules in `pack.json`.

```bash
# dry run first, then deploy and test it on the sample
./scripts/merionctl deploy examples/merion-agents/inbox-digest --dry-run
./scripts/merionctl deploy examples/merion-agents/inbox-digest --test
# or in the console: /app/keep → Deploy your own use case → paste pack.json (JSON tab)
```

## What it is, and is not

- **A digest built by rules.** It picks out lines that mention your keywords and counts what repeats.
  **No model reads your mail.** The export never leaves the sealed cell, and the run must report 0 CONNECT.
- **Plain text only.** Export your mail as plain text (`.txt`, `.eml` or `.mbox`). HTML-only, base64 or
  quoted-printable messages arrive as raw text and match poorly.
- **The first 300 000 bytes.** Larger exports are cut off. Split them by month.
- **Keywords, not understanding.** It does not sort into folders, group by thread or rank importance.
  Tune the keyword lists in `pack.json` to your own mail.
- **Repeated headers dominate the last list.** Every message repeats `Date:` and `To:`, so read the `From:` lines under them.
- **No mailbox connection.** You export; Keep reads the file. Live IMAP or SMTP access is not part of Keep.

`sample.txt` is a made-up export (example.com addresses only) so the use case runs in one click.
See [Tutorial 20](../../../docs/tutorials/20-sort-a-mail-export.md) and [Tutorial 19](../../../docs/tutorials/19-build-your-own-use-case.md).
