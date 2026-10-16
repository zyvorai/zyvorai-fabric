# Agent-proposed tools

## Why

A useful agent will often need something it does not have: a new small capability (call this API, parse that file, wrap a routine). If
every such tool needs the operator to write, sign and deploy it, the agent is only as capable as its last deploy. If the agent can add tools
to itself freely, it can add **authority** to itself, which is the thing Keep exists to prevent.

## What exists

- Agents are packs: a bundle plus a manifest (egress mode and allowed hosts, credentials the agent may name, resources, memory, model socket,
  approval timeout). In Keep mode a deploy needs a bundle **signed by a trusted signer** and a signed policy; the deploy route
  (`POST /v1/agents`) is the operator's, and a user token cannot reach it ([TENANCY.md](../TENANCY.md)).
- A credential an agent lists is checked against the vault at deploy; a use of it is still checked at the egress broker (host, method, path, port,
  approval, device signature).
- Sessions run in sealed cells; an agent can only propose memory entries, suggestions, plans and cards (events the host validates), never act
  on its own configuration.
- A session that read untrusted content is `tainted`, and taint marks what it proposes.
- Cards (`card.propose`, [card/README.md](../card/README.md)) are a separate, narrower mechanism added since this note was first written: a
  read-only display an agent can hand the host, gated on `manifest.card`. A card decides nothing and grants no authority — it is pure display,
  not capability. `tool.propose` below is unrelated to it: this proposal is about adding capability and the authority that comes with it, which
  cards never do.

## The principle

**A tool may add capability. It may not add authority.** Capability is code in a cell. Authority is what the cell can reach: hosts, credentials,
files, models, memory. The person's device, not the agent, grants authority, and only from a ceiling the operator set.

## Proposal

**A. A tool is a proposal, in an event.** An agent emits `tool.propose` with `{name, description, bundle, manifest}`. The host stores it as a
*tool proposal* for the session's person (bounded size and count, plain-text description, same guards as suggestions). It does nothing.

**B. The review shows the difference in authority, not just the code.** The person sees: the tool's description, the bundle's hash, and a
**diff against what the proposing agent already has**: new egress hosts, new credentials, memory access, a model socket, more resources,
longer timeouts. A tool that adds no authority (pure computation in the cell) is flagged as such. Code is shown as a hash and size (a person
cannot review it; the review that matters is the authority diff and where the tool came from).

**C. Approval is signed on the device and deploys into a user namespace.** Approval uses the existing device-signed decision. The host
deploys the tool as `u/<user>/<name>`, **clamped by a per-user ceiling policy** the operator set (allowed hosts and credentials a user may ever
grant a tool, resource caps, no model socket unless allowed). It can never exceed the ceiling, whatever the approval says. Versions are
immutable; a new version is a new proposal.

**D. Tainted authors get extra friction.** A tool proposed by a tainted session is marked, needs the explicit confirm accepting a tainted plan
already needs, and deploys with egress in `ask` mode until the person clears it.

**E. Revocation is one click, immediate, and journaled.** The journal records proposal, approval and deploy (names, hashes, authority
diff; never code).

## What I would refuse to build

- An agent that edits its own manifest or widens its own egress list in place.
- Auto-approval of "safe-looking" tools, including "no new authority" ones (the first version approves all; relaxing it can come later).
- A tool that can call the deploy route, sign anything, or mint tokens.
- Tools that ship inside a model's tool-call reply without going through a proposal.

## How I would verify it

Two users and an operator against the real router: a tool cannot exceed the ceiling however it is approved; another person cannot see or
approve a proposal (404); an unsigned or flipped approval is refused; the authority diff is right for added hosts, credentials, memory and
model sockets; a tainted proposal needs the confirm; revoke stops new sessions and is journaled; a deployed tool cannot reach the deploy
route. `demos-ci.sh` with a stub cell that proposes a tool.

## Size

Proposal storage, routes and diff: medium. Ceiling policy and the user namespace in the store and authz: medium (the delicate part).
Client screens: small each. Three to four PRs. It touches the deploy path, so it needs the most careful review of all five notes.

## Decisions for you

1. **Do you want agents to be able to extend themselves at all?** Recommendation: yes, but only through this proposal path, and only after
   payments and browser commit-approval exist, because those are what make wider authority dangerous.
2. **The ceiling:** what may a user ever grant a tool? Recommendation: hosts from an operator allowlist, credentials from the user's own
   connections, no model socket, small resources.
3. **Who signs the bundle** once a person approves it: a host key (so the trust is "this host deployed it") or the person's device key.
   Recommendation: a host key, with the person's signed approval recorded next to it.
