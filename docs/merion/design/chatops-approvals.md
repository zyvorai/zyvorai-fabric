# Chat approvals (Slack and Teams)

Built (tested against fakes; not yet run against a real Slack workspace or Teams tenant). Code: `agent-runtime/src/chatops.rs`.

## Why

Teams that gate agent actions on a person want the request where they already work. The risk is that a chat click is a weaker proof
than a phone signature, so chat must never become the way around the phone.

## What it does

- A new approval is posted to `ZYVOR_AGENT_SLACK_WEBHOOK` and/or `ZYVOR_AGENT_TEAMS_WEBHOOK` (https, or http on loopback).
- The message holds the approval kind, subject, agent name and a console link (`ZYVOR_AGENT_CONSOLE_URL`). It never holds the
  agent's `prompt`, a request body or the host-rendered preview. Fields are stripped of control characters, cut to 200 characters,
  and escaped for Slack.
- **No buttons when a phone must sign:** `merion_mode`, `ZYVOR_AGENT_REQUIRE_DEVICE_SIGNATURE=1`, or a credential with
  `require_device_signature`. The message carries a link only.
- **Buttons otherwise,** only if `ZYVOR_AGENT_SLACK_SIGNING_SECRET` and `ZYVOR_AGENT_SLACK_USERS` (JSON, Slack user id to Merion
  user id) are set. A click is verified (`X-Slack-Signature`, `v0` HMAC-SHA256, five-minute window), the Slack user mapped, the
  mapped user checked to own the session, and the decision made through `decide_approval` as a user token with only the `approve`
  scope. That path re-checks the signature rule, so a misconfigured message still cannot approve a phone-only approval.
- Teams gets an Adaptive Card with an open-link action. A Teams button needs a bot registration, so it is not offered.
- A failed post is retried twice and then journaled as `approval.chatops`.

## What it will not do

- Decide an approval that needs a phone signature.
- Show agent-written text in the message.
- Let a Slack user who is not in the operator's table, or who does not own the session, decide anything.

## Not verified

A real Slack app (interactivity URL, signing secret), a real Teams workflow webhook, and a click through the console link.
