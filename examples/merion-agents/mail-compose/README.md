# mail-compose

Writes a plain-text mail as a Gmail **draft** (the default) or **sends** it. Both wait for the person: the credentials need a decision signed by their phone key, and the approval shows the recipients, subject and text as the host read them out of the request ([what the person sees](../../../docs/merion/connectors/README.md#what-the-person-sees-before-they-approve)).

```
send                                  <- "draft" (the default) or "send"
to: ana@example.com, ben@example.com
subject: Lunch?

Are you free at noon?
```

Refuses, before any request: no or more than ten recipients, anything that is not a plain address (no `Name <a@b>`, no line breaks), a subject over one line or 200 characters, an empty or over-20000-character text. Deny it on the phone, or let it time out (at most 240 s), and the reply says it was not sent.

Run against a real Gmail account once (2026-09-27): a draft waited for the approver and landed in Drafts, and a denied send never reached Google. An approved send has only been run against the fake Google. See [what was verified](../../../docs/merion/connectors/README.md#verified-against-real-google).
