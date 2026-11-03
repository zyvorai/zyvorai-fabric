---
sidebar_position: 8
---

# Merion and personal-agent apps

*Written 2026-09-26. Personal-agent apps are a young, fast-moving category, so this page describes the layers and the trade-offs, not a
particular product. If something here is out of date, please open an issue.*

A personal-agent app is an **application**: a chat client, connectors to mail and calendar, task views, notifications. Merion is a
**runtime**: sealed cells, policy, a vault and device-signed approvals. They sit at different layers, so the useful question is what each
layer is good at and how they fit, not which is better.

## What such apps typically provide

- A client people can open on a phone or in a browser, usually a chat surface with rich result cards.
- Connectors to mail, calendar and documents, with a review step before an action is taken.
- Durable work: plans, pause and resume, goals, page tracking and alerts.
- A demo that runs on a laptop with no special hardware.
- Their agent computer is commonly a container on the same machine, which is simple to run and is not a boundary against a hostile tenant.

## What Merion provides

- **The agent's computer is a microVM** with its own kernel. The host applies a deny-all network policy before any guest work, and the run fails closed if
  it cannot be applied ([confine.md](confine.md), [THREAT-MODEL.md](THREAT-MODEL.md)).
- **Approvals bound to a device key**: signed over the exact text with a challenge and an expiry (Secure Enclave in Solvor), not an in-app button.
- **Tenancy**: users are scoped by token and the isolation is tested ([TENANCY.md](TENANCY.md)).
- **60+ ready use cases** for files people actually have, and a declarative way to add more ([CONTRIBUTING-PACKS.md](CONTRIBUTING-PACKS.md)).
- **Honest evidence**: the evidence class is `software-test`, not hardware-attested, and every surface says so ([SECURITY-PROFILES.md](SECURITY-PROFILES.md)).

## Where Merion is behind today

- **Setup.** Sealed cells need a Linux host with KVM. [`scripts/merion-up.sh`](../../scripts/merion-up.sh) is the one-command path (tested with fake facts; a clean-machine
  run is still open, see [TODO.md](TODO.md)), and [`scripts/merion-demo-local.sh`](../../scripts/merion-demo-local.sh) runs a clearly labelled, **not sealed** simulator on a laptop.
- **Clients.** The web console and Solvor (macOS 26), which now has an in-app chat client ("Agent Home"). No iOS or Android app.
- **Connectors.** A browser-tab email reader in Solvor, plus a first-party Google mail/calendar connector in the runtime (OAuth, per-person
  or shared credentials; see [connectors/README.md](connectors/README.md)), live-verified against real Gmail and Calendar.
- **Long-running personal work.** Sessions with steer, cancel, resume and hibernate, plus schedules and triggers, and a goal/plan interface
  in Solvor (a visual timeline for a proposed or accepted plan's steps).
- **Protocols.** An HTTP API, an MCP endpoint, signed webhooks, and AG-UI (`POST /v1/agui`, see [AGUI.md](AGUI.md); tried with the repo's own chat page, not yet with a third-party AG-UI client).

## How they fit

An app needs a computer for its agent; Merion is a sealed one. The direction is to make that easy without either side pretending to be the other:
an agent-UI protocol endpoint on the Merion runtime so chat clients can drive a Merion session, and an adapter that lets an app run its agent computer on Merion instead
of a local container. Neither exists yet ([TODO.md](TODO.md) and [ROADMAP.md](ROADMAP.md) track them). Nothing here has been agreed with any other project.
