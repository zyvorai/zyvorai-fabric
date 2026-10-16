# Cards: a rich display that isn't an approval

A read-only, host-cleaned display an agent can show — the piece a proactive agent needs to say "here is what I found," when a plain text line isn't enough. A card decides nothing and grants no authority; it is never an approval, and it never touches the [rule](../ROADMAP.md#what-we-will-not-add) that approvals never happen inside a chat.

```
agent (a suggestion.propose already emitted this run, say)  --card.propose-->  host cleans it  -->  keep.card in the AG-UI stream
                                                                                               \\--> also saved as an ordinary artifact
```

## How it works

1. **An agent whose manifest asked for it** (`"card": true`) emits `ctx.emit("card.propose", {kind, fields: [{label, value}, ...]})`. `kind` must be one the host recognises — today, exactly one: `suggestion-digest`, a finder's own "here is what I found" summary (a few items and why each is worth a look). Adding a second kind is a decision the host's code makes, not something an agent can invent by naming a new one.
2. **The host cleans it**: each `label` and `value` becomes plain text (control, zero-width and direction-changing characters removed, bounded length), 1 to 12 fields. Anything that doesn't fit this shape — an unknown kind, no fields, too many, an empty label — is refused (`card.refused` in the session's events, with the reason) rather than shown partially.
3. **It plays in the chat**: `CUSTOM keep.card` in the [AG-UI](../AGUI.md) stream, with `{kind, fields, artifact_id}`.
4. **It lives outside the chat too**: the same content is saved as an ordinary artifact (`kind: "card"`) in the artifacts store `POST /v1/artifacts` already backs, so it shows up wherever artifacts do — `GET /v1/artifacts/{id}`, Runs — not only in the one conversation that produced it.

## What a card is not

[Preview](../connectors/README.md) — the card shown before an approval (a mail's recipients and text, an event's time and guests) — is rendered by the host from the *actual outgoing request bytes*, so an agent cannot lie about what it is about to send: there is an independent ground truth to check it against. A card has no such thing. It is data the agent computed itself, with nothing outgoing to verify it against. The guarantee here is about **shape**, not **truth**: whatever an agent sends, a card can only ever become a bounded set of plain-text label/value pairs of a recognised kind — never raw HTML, a script, or an unbounded blob. It is not a way for an agent to prove something happened; it is a way for it to show something without that becoming an approval, a new authority, or an unbounded surface.

## Verified, and what is not

Unit-tested: `render` (a known kind, 1 to 12 bounded plain-text fields, an unknown kind or an empty label refused); `record_card` (a manifest that did not ask gets a refusal and nothing saved; a good card becomes both a `card.rendered` event and a retrievable artifact; a bad one becomes neither); the AG-UI mapping (`card.rendered` is its own `keep.card` event, distinct from the generic `keep.event` passthrough every other agent-emitted event gets). **Not built:** no example agent emits one yet (the natural next step is `calendar-suggestions` emitting its own digest alongside its `suggestion.propose` events); a second card kind, which needs its own decision.
