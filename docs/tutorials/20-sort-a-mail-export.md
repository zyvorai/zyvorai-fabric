# Tutorial 20: Sort and summarise a mail export

Tutorial 19 builds a use case for invoices. This one builds a **digest of your mail**: drop in an
exported mailbox and get what needs a reply, money, meetings, bulk mail and who writes most. It uses the
same no-code pack as before, so it takes a few minutes.

**Level:** Beginner  
**Time:** 10 minutes  
**Needs:** A Fabric + Keep runtime with FluxVM (`./scripts/deploy keep user@host` sets one up),
Node 20+ for the CLI, `cd sdk/agent-runtime && npm ci` once.

> **Honesty.** The digest is **extractive** and rule-based: it picks out lines that mention your keywords
> and counts what repeats. **No model reads your mail**, and nothing in the file runs. There is **no mailbox
> connection**: you export, Keep reads the file inside the sealed cell. Evidence class stays `software-test`,
> and zero CONNECT means the cell made no outbound connection, not "the operator cannot read the VM".

---

## What you get

For an export like [`sample.txt`](../../examples/merion-agents/inbox-digest/sample.txt) (a made-up mailbox,
`example.com` addresses only), the run produces `inbox-digest.md`. This is real output from the
runtime, trimmed:

```markdown
## Needs a reply
- Subject: Vendor renewal: can you confirm by Friday?
- Please reply with a yes or no so I can close it out.
- Urgent: I need them ASAP so I can print copies.

## Money
- Subject: Invoice INV-2041 is ready
- Amount due: 480.00, payment due in 14 days.
- Subject: Overdue: invoice INV-2029

## Meetings
- Subject: Meeting: Q4 planning, Wednesday
- Sharing the agenda for the Q4 planning meeting on Wednesday.

## Bulk and newsletters
- From: The Weekly Byte (no-reply@newsletter.example.com)
- Unsubscribe at any time.

## Who and what repeats most
- 3× From: Acme Billing (billing@example.com)
- 2× From: Daniel Okafor (daniel@example.com)
- 2× From: Priya Nair (priya@example.com)
```

In a mail export every message repeats its `Date:` and `To:` lines, so those top the "repeats most" list.
The `From:` lines under them are the ones to read.

## Step 1: export your mail as plain text

Export a month at a time, as plain text, from your mail client: `.txt`, `.eml` or `.mbox` all work.

- Keep the export **under 300 000 bytes**. A larger file is cut off at the limit, so split it by month.
- Prefer **plain-text** messages. HTML-only, base64 or quoted-printable bodies arrive as raw text and match
  keywords poorly.

## Step 2: deploy the pack and test it

```bash
./scripts/merionctl deploy examples/merion-agents/inbox-digest --dry-run    # what it would do
./scripts/merionctl deploy examples/merion-agents/inbox-digest --test       # deploy, run on the sample, require 0 CONNECT
```

You should see `test passed: inbox-digest.md, 0 CONNECT`. Or use the console: **Keep** (`/app/merion`) →
**Deploy your own use case** → paste `pack.json` (JSON tab).

## Step 3: run it on your export

In the console, choose **Inbox digest**, drop in your file and click **Run**. The cell comes up, the guest
reads the file, the digest is built, and the cockpit shows **0 CONNECT**. From the API:

```bash
curl -sf -X POST -F "file=@my-mail-september.txt" "$MERION_API/v1/demos/inbox-digest"
```

## Step 4: tune it to your mail

The rules are in [`pack.json`](../../examples/merion-agents/inbox-digest/pack.json). Each section is a
`keyword_sections` rule: a title, a list of keywords (case-insensitive), and how many lines to show.

```json
{ "kind": "keyword_sections", "title": "Needs a reply",
  "keywords": ["please reply", "can you", "let me know", "by friday", "urgent"], "max_lines": 8 }
```

Add the words your own mail uses ("PO number", "your order", a client's name), then run
`merionctl deploy examples/merion-agents/inbox-digest --test` again. Rules are bounded: up to 20 per use
case, no regular expressions and no commands. See [PACKS.md](../merion/PACKS.md).

## What this does not do

- **No live inbox.** Keep does not connect to a mailbox; there is no IMAP, SMTP or mail-provider sign-in
  in Keep today. You export, it reads.
- **No sorting into folders**, no grouping by thread, no ranking by importance. It groups lines by keyword.
- **No model.** The digest cannot summarise in its own words, and it cannot reply, send or delete anything.
- **Only the first 300 000 bytes**, and only text the extractor can read as plain text.

If you want a live mailbox, that is a custom agent pack (Tutorial 19, part 2) that you write against a
mail provider's HTTPS API, with the credential in the vault and every send or delete behind an approval.
It is a pattern the runtime supports, not something Keep ships.

## Next

- [Tutorial 18](18-merion-use-cases.md): the built-in use cases (contract clauses, meeting actions, log triage and more)
- [Tutorial 19](19-build-your-own-use-case.md): your own use case, then your own agent
- [PACKS.md](../merion/PACKS.md): the full `pack.json` reference
