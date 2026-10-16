# Google connectors (Gmail, Calendar)

**Status: the credential mechanism, the per-person connections, the approval previews and three example agents are built and tested against a fake Google (a fake token endpoint and a fake TLS API), and the first live run against real Gmail and Calendar worked on 2026-09-27, on a personal Gmail account (Google app in Testing mode, one test user), through `scripts/merion-demo-google.sh`: consent, the host minting a token from the person's own refresh token, reading real unread inbox headers, a draft that waited for the terminal stand-in phone and then appeared in Gmail Drafts, a send that was denied and never reached Google, and the calendar agenda ([what was and was not covered](#verified-against-real-google)).** Try it yourself: [GOOGLE_DEMO.md](GOOGLE_DEMO.md).

## How it works

A Keep agent never holds a Google token. The host holds the OAuth client id and secret and a long-lived **refresh token** in its own environment, mints short-lived **access tokens** from them, and adds `Authorization: Bearer ...` to the agent's request at the egress broker, after the usual checks (host, method, path, port, user, approval). The cell sees only a surrogate at most.

- Descriptor kind `oauth-refresh` in the credentials file (`ZYVOR_AGENT_CREDENTIALS_FILE`), with an `oauth` block: `token_url` (https; plain http only for loopback), `client_id_env`, `client_secret_env`, `refresh_token_env`, optional `refresh_margin_secs` (default 300).
- At start the runtime gets a first token for each such credential and then refreshes it in the background a few minutes before it expires (retrying with backoff, 15 s up to 5 min). If a refresh fails, the credential **fails closed** (requests using it are refused) until one works; the log names the credential and Google's error code (for example `invalid_grant`), never a secret.
- The token cache lives in memory only. The refresh token and client secret stay in the host environment, the same trust level as every other vault secret ([vault](../vault/README.md)): the operator of the host can read them.
- Google does not rotate refresh tokens in this flow. If you revoke the app in your Google account, the next refresh fails and the credential stays closed.

## Set it up

1. In the Google Cloud console, enable the Gmail API and Google Calendar API, configure the consent screen (add yourself as a test user), and create an OAuth client of type **Desktop app**.
2. Get a refresh token once, on your own machine:

   ```bash
   GOOGLE_CLIENT_ID=... GOOGLE_CLIENT_SECRET=... scripts/merion-google-auth.py            # read-only mail and calendar
   GOOGLE_CLIENT_ID=... GOOGLE_CLIENT_SECRET=... scripts/merion-google-auth.py --with-drafts   # also Gmail drafts
   GOOGLE_CLIENT_ID=... GOOGLE_CLIENT_SECRET=... scripts/merion-google-auth.py --with-send --with-events   # also send mail and create events
   ```

   It opens your browser for consent (authorization code with PKCE on a loopback port) and writes `GOOGLE_REFRESH_TOKEN=...` to `google-refresh-token.env` with mode 0600. The token is not printed.
3. Put `GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET` and `GOOGLE_REFRESH_TOKEN` in the Keep host's environment and merge [google.credentials.json](google.credentials.json) into the credentials file.
4. In an agent's manifest, list the credentials it may use, for example `"credentials": ["gmail-read", "calendar-read"]`, and call `https://gmail.googleapis.com/gmail/v1/users/me/messages` or `https://www.googleapis.com/calendar/v3/calendars/primary/events` through the egress broker.

## Per-person connections (one Google account for each person on the host)

The setup above gives the whole host one Google identity. On a host with several people ([TENANCY](../TENANCY.md)) each person connects their **own** account instead:

1. The operator keeps only the OAuth **client** in the host environment (`GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET`) and merges [google.per-person.credentials.json](google.per-person.credentials.json) into the credentials file. Those descriptors set `"connection": "google"` in place of `refresh_token_env`; a descriptor sets one or the other.
2. Each person runs `scripts/merion-google-auth.py` on their own machine, then stores the token with their own user token:

   ```bash
   curl -X PUT "$KEEP/v1/connections/google" -H "Authorization: Bearer $USER_TOKEN" \
        -H 'content-type: application/json' \
        -d "{\"refresh_token\": \"$(sed 's/^GOOGLE_REFRESH_TOKEN=//' google-refresh-token.env)\"}"
   ```

   `GET /v1/connections` lists the connections this host offers and whether *you* have set each (never the token). `DELETE /v1/connections/google` disconnects: the stored token and every cached access token of that person are dropped at once.
3. When an agent of that person's session calls Gmail, Keep mints an access token from **that person's** refresh token and injects it. Another person's session never gets it, a session with no user cannot use a per-person credential at all, and a person who has not connected gets a refusal that says to connect first.

What this does and does not do: the refresh token is write-only over the API (never returned, listed, logged or journaled; the journal records only that a connection was set, removed or accessed by the operator) and is held in a host file (mode 0600), one per person. As with the vault, **the operator of the host can read those files**; per-person connections separate people from each other, not from the operator. A name can be set only if some credential on the host asks for it.

## The example credentials

| Name | Allows | Approval |
|---|---|---|
| `gmail-read` | `GET` under `/gmail/v1/users/me/` | none |
| `gmail-draft` | `POST` to exactly `/gmail/v1/users/me/drafts` (creates a draft; not `/drafts/send`, so it cannot send one) | every use needs a decision signed by the person's phone key; the host shows the message first |
| `gmail-send` | `POST` to exactly `/gmail/v1/users/me/messages/send` | the same |
| `calendar-read` | `GET` under `/calendar/v3/` | none |
| `calendar-write` | `POST` to exactly `/calendar/v3/calendars/primary/events` (creates an event; no edit or delete) | the same, and the host shows whether the guests are emailed |

An entry in `path_prefixes` that ends in `$` matches that exact path only. (An earlier version of `gmail-draft` listed the plain prefix `/gmail/v1/users/me/drafts`, which also matched `/drafts/send`, contrary to what this page said; it is now exact, and a request that is not a renderable message is refused anyway, see the next section.)

The scopes match: the auth script asks for `gmail.readonly` and `calendar.readonly` by default and adds `gmail.compose` with `--with-drafts`, `gmail.send` with `--with-send` and `calendar.events` with `--with-events`. Scopes are Google's limit; the descriptor's method and path lists are Keep's, and both apply. Ask only for what you use: `gmail.compose` also lets Google send mail, so with it the only thing between an agent and a send is Keep's path list and the approval.

## What the person sees before they approve

An approval that says only "POST to gmail.googleapis.com, 812 bytes" asks the person to sign something they cannot read. A descriptor can set `"preview": "gmail-message"` or `"calendar-event"` (only with `requires_approval`). The host then reads the **actual request body** and renders it into the approval, so an agent cannot describe one thing and send another:

- Mail: every `To`, `Cc`, `Bcc` (labelled hidden), `Reply-To` and `From` header, each repeat of one, the subject with encoded words decoded, the names of any other header, and the first 1500 characters of the text.
- Event: title, start and end (with zone, or all-day), guests, place, repeat rule, the notes, any other field it sets, and whether the guests are emailed (that depends on `sendUpdates` in the URL, which the approval's own URL leaves out).

The rendering is on the approval (`preview`, in `GET /v1/inbox` and `GET /v1/approvals` for its owner) so the phone can show it, and `planned_action.preview_sha256` is a digest of it, which the phone's signature therefore covers. Three rules keep it honest:

- **Fail closed.** A body the host cannot render faithfully is refused with 422 before anything is sent: HTML or multipart mail, attachments, another character set or transfer encoding, JSON that is not what the API takes, a `drafts/send` body with no message in it. Only plain-text (7bit or 8bit, UTF-8 or ASCII) mail can be approved for now.
- **Plain text.** Values are length-limited and stripped of control, zero-width and direction-changing characters, so a subject cannot display differently from what it is.
- **Not on the permanent record.** The rendering is dropped when the approval is decided or expires. It is never written to the audit journal, an operator webhook or a push relay; those carry the generic prompt and the digest only. Hence a relay or the journal learns that *something* was sent, not to whom.

## The example agents

`examples/merion-agents/`: no model, no network beyond Google, each granted only the credentials it needs.

| Agent | Does | Credentials |
|---|---|---|
| [`gmail-triage`](../../../examples/merion-agents/gmail-triage/) | lists unread inbox mail (sender, subject, date) | `gmail-read` |
| [`mail-compose`](../../../examples/merion-agents/mail-compose/) | saves a plain-text mail as a draft (the default), or sends it; refuses anything that could add a header or hide a recipient | `gmail-draft`, `gmail-send` |
| [`calendar-agent`](../../../examples/merion-agents/calendar-agent/) | lists your next 24 hours (up to 14 days), or adds one event; guests are emailed only when asked and only if there are any | `calendar-read`, `calendar-write` |

**To try them on your own Gmail in a few minutes, see [GOOGLE_DEMO.md](GOOGLE_DEMO.md).** Talk to them from the chat page (`scripts/merion-chat.py --agent mail-compose`) or set structured input (`action`, `to`, `subject`, `body`; see each `agent.ts`). A refusal from the host (no Google account connected yet, denied on the phone, no answer within `egress_approval_timeout_seconds`, at most 240) is said in the reply instead of failing the run. The inputs `gmailBase` and `calendarBase` exist so tests can point an agent at a fake Google; the credential's host binding means a real credential still goes only to Google.

## Verified, and what is not

- Unit tests (`cargo test --lib credentials`): the refresh grant is sent with the client id, secret and refresh token; the token is cached and injected as `Bearer`; a refusal (`invalid_grant`) keeps the credential closed and leaks no secret into the error; an expired token is no longer used; the startup refresh; descriptor validation (https only, required fields); the method and path policy still applies.
- `agent-runtime/tests/merion-google-auth-test.py`: the consent script against a fake Google: PKCE and offline-access parameters, read-only scopes by default, drafts only with the flag, the code exchange, a wrong `state` or a refusal aborts, the file is mode 0600, and the refresh token is never printed.
- `cargo test --lib preview`, `egress`, `credentials`, `notify`: the renderer (recipients incl. Bcc and repeats, encoded words, control and direction characters, refusal of HTML, multipart, other encodings, oversize; events, guests, `sendUpdates`, malformed events); through the real egress path, the approval carries the rendering of the real body and a digest of it, the text is absent from the prompt, the planned action, the audit journal and the approvals file once decided, and an unrenderable body is refused (422) with no approval opened and nothing sent; a webhook and a relay payload never carry it; exact-path (`$`) matching; descriptor validation. The shipped example descriptor files are loaded and validated by a test.
- `sdk/agent-runtime/test/google-agents.test.js`: each agent against a fake `ctx.fetch`: which URL and credential it uses, that mail defaults to a draft, that anything that could add a header or hide a recipient is refused before any request, that guests are emailed only when asked, and how it reports a refusal from the host.
- `agent-runtime/tests/demos-ci.sh` (real runtime, real per-person credentials, real TLS to a fake Google over a throwaway CA, the deployed agents, a real phone key): before connecting the agent is told to connect and nothing reaches Google; gina's calls carry her token and hal is refused; a draft opens an approval showing the real recipients, subject and text; an unsigned decision is refused; the signed one lets exactly that draft through; the decided approval keeps no copy and the journal never held one; a denied send never reaches Google and an approved one does, once; an event shows its guests and that they are emailed; disconnecting closes her at once.
- **Real Google:** see [Verified against real Google](#verified-against-real-google) below for what one live run covered and what it did not.
- **Also not tested:** a real phone app rendering the preview (the fields are there in the API; no client shows them yet), and HTML or multipart mail, which are refused rather than shown.

## Microsoft 365 (Outlook mail and calendar)

Same shape as Google: a credential the host injects, per person, with writes decided on the phone against a preview the host rendered. What differs is Microsoft's OAuth, and the descriptors say so.

- **A public client, no secret.** You register an app (Microsoft Entra admin center, "App registrations") as a *public client* with the redirect URI `http://localhost` and the delegated Graph permissions you will use. Its Application (client) ID goes in the host's `MICROSOFT_CLIENT_ID`. `client_secret_env` is empty in the descriptors, and no `client_secret` is sent (Microsoft refuses an empty one).
- **Refresh tokens rotate.** Every refresh returns a new refresh token. Keep stores it (per person, only if the connection still holds the token that was used, so a disconnect or a re-connect meanwhile is never undone). That is why there is **only a per-person descriptor file** for Microsoft, [microsoft.per-person.credentials.json](microsoft.per-person.credentials.json): a host-wide token lives in the environment and cannot be replaced, and Keep logs a warning if a host-wide credential is ever rotated.
- **Narrow tokens.** The new `oauth.scope` field is sent in the refresh request, so each credential's access token carries only what it needs even though you consented to more: `outlook-read` gets `Mail.Read`, `outlook-send` gets `Mail.Send`, and so on (plus `offline_access`).
- **Sign in:** `MICROSOFT_CLIENT_ID=... scripts/merion-microsoft-auth.py` (`--with-drafts`, `--with-send`, `--with-events`, `--tenant`), then `PUT /v1/connections/microsoft` with the token, exactly like Google's ([per-person connections](#per-person-connections-one-google-account-for-each-person-on-the-host)).

| Name | Allows | Approval |
|---|---|---|
| `outlook-read` | `GET` under `/v1.0/me/messages` and `/v1.0/me/mailFolders` (`Mail.Read`) | none |
| `outlook-draft` | `POST` to exactly `/v1.0/me/messages` (creates a draft; not `/messages/{id}/send`) (`Mail.ReadWrite`) | signed by the phone; the host shows the message |
| `outlook-send` | `POST` to exactly `/v1.0/me/sendMail` (`Mail.Send`) | the same |
| `outlook-calendar-read` | `GET` `/v1.0/me/calendarView` and under `/v1.0/me/events` (`Calendars.Read`) | none |
| `outlook-calendar-write` | `POST` to exactly `/v1.0/me/events` (`Calendars.ReadWrite`) | signed by the phone; the host shows the event |

**Previews** (`graph-message`, `graph-event`) read the real Graph request body like the Gmail and Calendar ones do: recipients with the address first (the display name is free text and cannot pose as it), Bcc labelled hidden, subject and the first 1500 characters; for an event, title, times with zone, place, guests and any recurrence. **Refused instead of shown:** HTML bodies, attachments, anything next to the message that cannot be reviewed; fields it does not render are named ("Also sets"). Unlike Google there is no switch for guest emails: **Microsoft emails every guest when an event is created**, and the preview says so.

**Agents:** [`outlook-triage`](../../../examples/merion-agents/outlook-triage/), [`outlook-compose`](../../../examples/merion-agents/outlook-compose/) (draft by default, or send) and [`outlook-calendar`](../../../examples/merion-agents/outlook-calendar/) (agenda in UTC, or add one event; guests are only added with `notify: yes`, because they are emailed).

**Verified, and what is not.** Unit tests: the refresh request carries the scope and no secret; a rotated token is handed back only when it changed; `ConnectionStore::rotate` (replaces only the token that was used, never revives a disconnected person, keeps `connected_at`, survives a restart); through the real egress path the rotated tokens are stored per person and a rotation does not bring back someone who disconnected; the Graph previews (recipients, name spoofing, Bcc, HTML, attachments, unknown fields, events and guests); the shipped descriptor file is loaded and checked (per person, public client, one Graph permission each, exact paths for writes, a draft credential cannot send a draft). `merion-microsoft-auth-test.py`: the consent script against a fake Microsoft (PKCE, `localhost` redirect, default and wider scopes, no secret, wrong state aborts, mode 0600, token never printed). 8 Node tests for the agents. `demos-ci.sh` with a fake Microsoft Graph over TLS: told to connect first with nothing sent; her mail and agenda read with her own token while another user is refused; the token endpoint saw the narrow scopes and no secret, and the **second mint used the refresh token the first one rotated to**; a draft approval shows the real Graph message, an unsigned decision is refused, the signed one lets exactly that draft through, the journal holds no recipients or text; a denied send never reaches Graph and an approved one does, once; an event with guests is refused until you allow the email, then shows who Microsoft will email. **Not tested (and parked: the live check needs an Entra directory, see [TODO](../TODO.md)):** anything against real Microsoft (consent, the real token response, Entra's app-registration behaviour, throttling, work or school tenants that restrict consent), and `$filter`/`$orderby` on real Graph, which I wrote from its documentation.

## Verified against real Google

One live run, on 2026-09-27, by the owner on a personal Gmail account, using [`scripts/merion-demo-google.sh`](GOOGLE_DEMO.md) (a Google Cloud project in **Testing** mode with one test user, a Desktop OAuth client, the local simulator for the cell and a software key for the phone). It worked:

- Google's consent page (with the unverified-app warning) and the code exchange; the runtime then held the refresh token as that person's own connection.
- **Reading:** `gmail-triage` listed real unread inbox headers, with a token the host minted from the stored refresh token.
- **A draft behind an approval:** `mail-compose` opened an approval showing the real recipient, subject and text (rendered by the host); the terminal stand-in phone signed the decision and the draft appeared in Gmail Drafts.
- **A denial:** a send answered `n` never reached Google.
- **Calendar:** `calendar-agent` read the agenda.

**Not covered by that run** (still only tested against the fake Google): an *approved send*, *creating a calendar event*, a second person connecting their own account, the refresh token being renewed over time (Google expires it after 7 days for an app in Testing), an app past Google's verification, a Google Workspace account, and a real phone (the demo's key is a file on the laptop, so it shows the signature flow but not the Secure Enclave's protection).

The runtime, the credentials and the approval code were unchanged for the run. Where the fake Google and the real one could have differed (scope names, response shapes, the token response), nothing needed changing on this run.

## A plain API-key connector, with no OAuth at all: price-watch

Google and Microsoft above are the involved case — OAuth, refresh tokens, per-person connections, host-rendered previews. Most services need none of that: `CredentialDescriptor`'s default `kind` (`"provider"`, used implicitly by `anthropic` and `openai` in [`credentials.example.json`](../../../agent-runtime/credentials.example.json)) is a plain API key injected into one header, and it needs **no Rust code at all** to add — only a JSON descriptor, an env var, and an agent that lists it.

[`price-watch.credentials.json`](price-watch.credentials.json) is the smallest possible example:

```json
{
  "price-watch-read": {
    "host": "api.pricewatch.example",
    "header": "x-api-key",
    "env": "PRICE_WATCH_API_KEY",
    "allowed_methods": ["GET"],
    "path_prefixes": ["/v1/"]
  }
}
```

`api.pricewatch.example` is a placeholder (the reserved `.example` domain, RFC 2606) — this is not an integration with any real vendor. To point it at a real price-tracking API you use, change `host`, `header` and `path_prefixes` to match that API and put its key in `PRICE_WATCH_API_KEY`; nothing else changes.

[`calendar-suggestions`](../../../examples/merion-agents/calendar-suggestions/) (above, [suggestions](../suggestions/README.md)) is the "keeps working" half of a personal agent; [`price-watch`](../../../examples/merion-agents/price-watch/) is the "connects to anything" half — a `suggestions: true` agent that checks a few tracked items against a target price and proposes one when it drops, meant to run on the same kind of schedule. **Tested against a fake `ctx.fetch` only** (`sdk/agent-runtime/test/price-watch.test.js`), the same way every example agent's own logic is unit-tested; nothing here was run against a real price API, because there is no real vendor behind it to run against.

