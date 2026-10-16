# Mail approvals for real-world mail (HTML, attachments)

## Why

Today only **plain-text** mail can be approved: the host reads the request body and shows the recipients, subject and text
([connectors](../connectors/README.md#what-the-person-sees-before-they-approve)). HTML mail, multipart mail and attachments are **refused**
before anything is sent. That is safe, and it means most real mail (newsletters, invoices, anything with a signature block or a file) cannot be
sent by an agent at all.

## What exists (read from `preview.rs`)

- Gmail: the raw RFC 822 message is base64url-decoded and parsed; `Content-Type` must be `text/plain` (UTF-8 or ASCII), transfer encoding 7bit or 8bit.
  Graph: `body.contentType` must be `Text`, and `attachments` is refused.
- The rendering is host-made from the real request, length-limited, stripped of control and direction-changing characters, kept only while the
  approval is pending, kept out of the journal, and covered by the signature through `preview_sha256`.
- Anything it cannot render faithfully fails **closed** (422, nothing sent).

## The risk that made refusing the right first answer

An approval is only worth something if what the person reads is what the recipient gets. HTML breaks that in ways plain text does not:
text hidden with CSS, links whose visible text differs from their target, tracking images and remote content, look-alike domains,
markup that renders differently in different mail clients, and attachments whose names lie about their type or that carry
active content. A preview that "renders the HTML like a mail client would" is itself a large attack surface on the host.

## Proposal

**A. Approve the bytes, show a faithful reduction.** The digest the phone signs covers the **exact bytes** that will be sent (the whole
message, HTML and attachments included), so nothing can change after approval. What the person *reads* is a deterministic reduction made in
a sealed renderer (below), presented with the differences that matter called out.

**B. The reduction for HTML** (deterministic, no browser engine):
- visible text only, in reading order, with hidden content removed **and counted** ("3 hidden elements removed");
- every link as `visible text → real target (host highlighted)`, and a warning where the two hosts differ or the target is an IP, a
  punycode or a look-alike of a host the person has mailed before;
- images listed by source; any **remote** image flagged as tracking ("this loads from another server when opened");
- forms and scripts flagged, and a message with them **refused**;
- the plain-text alternative part, if any, shown alongside, and a warning if the two disagree materially.

**C. The reduction for attachments:** name, declared type, **detected type** (by content, not extension), size and SHA-256 computed by
the host; a type allowlist (documents, images, archives without executables) with everything else refused; a text preview only for
text and PDF, produced in the renderer. An attachment whose detected type differs from its name is flagged and refused by default.

**D. The renderer is a sealed cell, not host code.** The host sends the message bytes to a short-lived cell (the same isolation Keep
uses for extractors), gets back the reduction as data, and the cell has no network. A parser bug then lands in a cell that is thrown
away, not in the runtime. The host still verifies the reduction is well-formed and bounded before showing it.

**E. Fail closed remains.** Anything the renderer cannot reduce (malformed MIME, nested archives, encrypted parts, a size over the limit)
is refused with the reason.

## What I would refuse to build

- Rendering HTML with a real browser engine on the host, or in the person's mail client, to "preview" it.
- Approving a message by a hash of the *reduction* alone (the signed digest must cover the bytes actually sent).
- Sending an attachment the host has not typed and hashed itself.
- Letting the agent tell the host what type an attachment is.

## How I would verify it

A corpus of hostile messages in tests (hidden text, mismatched links, look-alike domains, remote images, scripts, forms, a `.pdf` that is an
executable, a zip bomb, malformed MIME) each asserting the reduction and the refusal; the signed digest changes if a single byte of the HTML
or an attachment changes; the renderer cell has no network (the same check the extractors have). Then a real run on Gmail and Outlook
by you, the same way as the plain-text one.

## Size

Renderer cell and reduction: large (the security-critical part). Provider adapters (Gmail multipart, Graph attachments) and the preview
fields: medium. Three to five PRs, and it needs a hostile-message corpus first.

## Decisions for you

1. **Is HTML worth the risk now?** The alternative is plain text plus attachments. Recommendation: **attachments first** (invoices, documents,
   the common need), HTML second.
2. **Attachment types you actually need** (documents, images, archives?). This decides the allowlist.
3. **A size limit** (Gmail's limit is 25 MB; a lower ceiling for an agent is safer). Recommendation: 10 MB.
4. **Do you accept refusing anything the renderer cannot reduce**, even if it means some real mail cannot be sent by an agent?
