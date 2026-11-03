# Browser workflows: takeover and confirm-before-submit

## Why

Meta's Muse announcement lists browser automation, forms, travel and shopping as available. For Keep the same job is: the agent works a website
for you (find, fill, compare), and a person stays in control at the moments that matter: taking the wheel, and pressing the button that
sends or spends.

## What exists today (read from the code)

- A brokered Chromium in the cell. The model proposes `open`, `snapshot` and `act`; Chromium, CDP, cookies and the proxy stay host objects
  ([BROWSER-0.3.md](../browser/BROWSER-0.3.md), [DRIVER.md](../browser/DRIVER.md)). An accessibility-tree driver, **not pixels**.
- **Read-only live view:** tab titles and URLs, a screenshot (rate-limited) and a frames-only screencast. **No input takeover.**
- **Split-sight pause:** `POST …/agent-pause` and `…/agent-resume` stop the agent's tools (reasons `vault_fill`, `operator_watch`, `taint`).
- **Origin taint** across tabs and the clipboard (a paste across disjoint hosts is denied) and per-session limits (`max_origins_per_hour`,
  `max_checkout_asks`).
- **Vault-typed fill:** a password is filled by the host after the vault allows it; the value never reaches the model.
- **A witness scaffold** (`witness_vote` in `browse_ifc.rs`): it denies a click whose label contains buy, purchase, pay, submit or confirm
  **only if that label is absent from the last snapshot**. It is a keyword heuristic, not a check that a person agreed.
- Goal-bound tabs (`goal.allow_hosts`), trajectory-as-code (a replayable script), two cookie jars (the operator's blocks agent tools).
- Honesty stays `software-test`.

## The gap

1. A person cannot take over: type a password or a code themselves, solve a captcha, or fix a mistake, then hand back.
2. "Confirm before submit" is a keyword guess. It does not know that *this* click will send an order or a message, so a button labelled
   "Continue" can spend money and a harmless "Pay attention" link is blocked.
3. Nothing binds an approval to what the page will do: the person approves "click Place order" without seeing the order.

## Proposal

**A. Takeover as a lease, not a tunnel.** The person's device (or the chat page) asks for a *takeover lease* on one session
(`POST /v1/sessions/{id}/browser/takeover`). While it is held: the agent's tools are paused (the existing `operator_watch` pause), the person's
input goes through the host to that one tab over CDP input events, a visible banner shows on the live view, and every input is journaled as
counts (not keystrokes). Releasing the lease resumes the agent. The lease has a short timeout and ends when the session ends. A password typed
during takeover is **never** logged or sent to the model; the model sees only that a takeover happened (the snapshot afterwards is re-labelled
untrusted, since the person's typing changed the page).

**B. Confirm-before-submit that knows what is being submitted.** Replace the label guess with a *commit detector*: the host recognises a
commit by its effect, not its label:
1. a form submission or a click on an element that sits inside a form with a non-GET method;
2. a navigation the driver marks as leaving to a different origin after a form post;
3. a request the egress broker sees as a state-changing method to an origin the session has posted to.
When the agent's `act` would commit, the host **holds it and opens an ordinary approval**, with a preview the host builds from the page
itself: the form's action origin and path, its visible field labels and values (passwords and card fields shown as "hidden field"), and
the page title and URL. The person decides on their phone, signed, exactly like a send. The approval covers that one commit (its digest
includes the field values). If the page changes between the approval and the click, the digest no longer matches and the commit is refused.

**C. Sites the person has approved before.** A per-site *standing consent* is deliberately not proposed: it is how an approved habit
becomes an unreviewed spend. If it is wanted later, it should be an explicit, revocable, amount-bounded grant, designed with
[payments](payments.md).

## What I would refuse to build

- Any takeover that lets the agent see the person's keystrokes, or a shared session where both act at once.
- Approving from the chat or from inside the page.
- Screenshots or DOM sent to a model without the taint marking; a takeover that clears the taint.
- Autofilling saved cards or passwords from the person's own browser profile.

## How I would verify it

Against a local test site the repo controls (a page with a form that posts to a stub): commit detection for a post, a fetch and a
same-origin navigation; the preview shows the real fields and hides secret ones; an unsigned or flipped decision is refused; a changed
page invalidates the approval; takeover pauses the agent, journals counts only and cannot be started by the agent; a lease times out. Then one
real run on a real site the owner picks. Honest limits stay: an accessibility-tree driver misses what only pixels show.

## Size

Takeover: medium (CDP input, lease, banner). Commit detector and preview: medium to large; it is where the risk is. Two to four PRs.

## Decisions for you

1. **Takeover first, or commit-approval first?** Recommendation: commit-approval first. It closes the dangerous gap; takeover is convenience.
2. **Where does the person take over from?** The chat page (works today on a laptop), the iPhone app, or both? Recommendation: chat page first.
3. **Which site do we prove it on?** One real, low-stakes site of yours (a form that only sends you an email is ideal), so the first real
   run cannot spend money.
4. **Standing consents:** confirm that there are none in the first version.
