# gmail-triage

Lists your unread inbox mail (sender, subject, date) using the read-only `gmail-read` credential. No model. What it prints is text from other people and it does nothing with it.

Needs a Google connection: see [Google connectors](../../../docs/merion/connectors/README.md). Not connected yet? The reply says "connect your google account first". Try it from the chat page: `scripts/merion-chat.py --agent gmail-triage` with a user token.

Run against a real Gmail account once (2026-09-27): it listed the real unread headers. See [what was verified](../../../docs/merion/connectors/README.md#verified-against-real-google).
