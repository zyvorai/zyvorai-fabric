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
| [Model gateway](model-gateway.md) | Can agents get provider failover, a cache, and per-key spend and budgets without a key ever reaching a cell? |
| [Chat approvals](chatops-approvals.md) | Can Slack and Teams tell you about an approval, and let you decide it, without weakening the phone signature? |
| [Run report](run-report.md) | Can a finished run say what was checked, what was not, and how to roll back, without sounding more certain than the evidence? |
| [OTLP export](otlp-export.md) | Can the journal reach an OpenTelemetry collector without sending anything the SIEM export would not? |
| [Sequence policy](sequence-policy.md) | Can a policy stop an agent that has already made too many writes in one session? |
| [MCP gateway](mcp-gateway.md) | Can an operator register tool servers and limit which tools an agent calls, how often, with a record, without a second policy engine? |
| [Session recording](session-recording.md) | Can a person replay what a session did from a signed record that leaves out the content? |

How to read a note: **Decisions for you** at the end of each is the part to answer. Answer in a line ("1: yes, 2: A, 3: not now") and I will
turn the answers into a build plan. Anything you leave open stays open.

Rules that every note assumes (they are already how Merion works, and none of the proposals bends them):

- An approval is decided on the person's own device with a signed decision, **never in the agent chat** and never by the agent.
- The host, not the agent, renders what is being approved, from the real request.
- A secret (card, password, token) is never given to an agent or a cell; the host injects it and only where the descriptor allows.
- Anything that reads untrusted content marks the session tainted, and a tainted session's actions need a person.
- Nothing is public, signed with a real certificate, notarized or posted without your explicit go.
