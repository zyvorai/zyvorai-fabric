---
sidebar_position: 2
---

# Design notes for what needs a decision first

[REMAINING.md](../REMAINING.md) lists items that need a design decision before they are code. These notes are **proposals to decide on, not
built features.** Each one says what exists today (checked against the code), what the risk is, what I would build, what I would refuse to
build, and the decisions only you can make, with my recommendation for each. Nothing here has been started, except
[Credential sources](credential-sources.md), which was decided and then built (tested against fakes; no real Vault has been tried).

| Note | The question |
|---|---|
| [Browser workflows](browser-workflows.md) | How does an agent drive a website, with you able to take over and with a real confirm-before-submit? |
| [Payments](payments.md) | How can an agent buy something without ever holding a card number? |
| [Mail approvals for real-world mail](mail-approvals.md) | How do HTML and attachments get approved without approving what you cannot see? |
| [Agent-proposed tools](agent-tools.md) | How can an agent add a capability to itself without adding authority to itself? |
| [A Windows companion](windows-companion.md) | What would a Windows client be, and what must it never do? |
| [Credential sources](credential-sources.md) | Can a secret come from Vault or a mounted file instead of the process environment, without weakening any check? |

How to read a note: **Decisions for you** at the end of each is the part to answer. Answer in a line ("1: yes, 2: A, 3: not now") and I will
turn the answers into a build plan. Anything you leave open stays open.

Rules that every note assumes (they are already how Keep works, and none of the proposals bends them):

- An approval is decided on the person's own device with a signed decision, **never in the agent chat** and never by the agent.
- The host, not the agent, renders what is being approved, from the real request.
- A secret (card, password, token) is never given to an agent or a cell; the host injects it and only where the descriptor allows.
- Anything that reads untrusted content marks the session tainted, and a tainted session's actions need a person.
- Nothing is public, signed with a real certificate, notarized or posted without your explicit go.
