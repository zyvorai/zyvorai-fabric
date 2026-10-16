# Try Keep on your own Gmail and Calendar (a demo on your laptop)

`scripts/merion-demo-google.sh` runs the [Gmail and Calendar agents](README.md) against **your** Google account, on this machine, in a few minutes. It is a demo, not a deployment: the agent's cell is a simulator (no VM, no network policy) and the "phone" is a key file on this machine. What is real is the part that matters for trust: Keep, not the agent, holds your Google token; the host mints short-lived access tokens; and a draft, a send or a new event waits for your decision, with the recipients, subject and text that **the host read out of the request**.

## What you do in Google (about 10 minutes, in your own browser)

I cannot do this part for you, and the client id and secret are for you alone; nobody else needs them.

1. Open the [Google Cloud console](https://console.cloud.google.com/) and create a project (any name, for example "Keep demo").
2. **APIs & Services → Library:** enable the **Gmail API** and the **Google Calendar API**.
3. **OAuth consent screen** (Google Auth Platform → Audience/Branding): user type **External**, publishing status **Testing**, and add **your own Google address as a test user**. Fill the app name and your email where it asks.
4. **Credentials → Create credentials → OAuth client ID:** application type **Desktop app**. Copy the **client ID** and the **client secret**.

Use a personal Gmail address if you can. A Google Workspace account (an address on your own domain) can work, but an administrator may block third-party OAuth apps.

## Run it

```bash
./scripts/merion-demo-google.sh --client-json ~/Downloads/client_secret_XXXX.apps.googleusercontent.com.json
```

That is the JSON Google lets you download when you create the client; the script reads the id and secret from it itself and never prints them. Or export them yourself:

```bash
export GOOGLE_CLIENT_ID=...apps.googleusercontent.com
export GOOGLE_CLIENT_SECRET=...
./scripts/merion-demo-google.sh
```

The downloaded file holds the client secret in plain text: keep it out of the repository and delete it when you are done (you can always download it again from Credentials, or reset the secret there).

- Your browser opens Google's consent page. Because the app is in Testing and unverified, Google shows a warning ("Google hasn't verified this app"): choose your test account, **Advanced → continue**. It asks for read access to mail and calendar, and, for the demo, drafts, sending and creating events. Each of those three stays behind an approval here.
- The script prints three chat addresses (mail reading, composing, calendar) and this terminal becomes the approver.
- **Read your unread mail:** open the first address and say `unread`. That reads your real inbox headers (sender, subject, date), and nothing is gated.
- **Draft a mail:** in the second address say
  ```
  draft
  to: you@example.com
  subject: Hello from Keep

  A test message.
  ```
  The chat shows "Waiting for your phone" with the recipients, subject and text. The terminal shows the same and asks; answer `y`. The draft appears in your Gmail Drafts. Nothing is sent. Use `send` in place of `draft` to send it, and try answering `n` to see that a denied send never reaches Google.
- **Calendar:** in the third address say `agenda`, or add an event (the [agent README](../../../examples/merion-agents/calendar-agent/README.md) has the format). Guests are only emailed if you say `notify: yes`.

Ctrl-C stops everything and deletes the temporary state. **Revoke access** any time at <https://myaccount.google.com/permissions>. The consent step writes `google-refresh-token.env` in the directory you ran it from (mode 0600): delete it when you are done. It is listed in `.gitignore` (with `microsoft-refresh-token.env` and `client_secret_*.json`) so `git add -A` cannot pick it up; GitHub's push protection also refused a push that had it in a commit.

## What to expect, honestly

- While the app is in Testing, Google expires the refresh token after **7 days**; run the consent step again then.
- The demo has worked end to end on a real Gmail account ([what was covered](README.md#verified-against-real-google)); an approved send and creating a calendar event are the parts that run has not exercised yet. If a step fails, the chat says why (Google's error code, never a token), so the message is safe to share.
- **"Access blocked ... has not completed the Google verification process" (error 403 access_denied)** means the Google address you signed in with is not in the app's **Test users** list (Google Auth Platform → Audience). Add it and run the script again.
- The terminal approver is not a phone: the key file is on this machine, so it shows the flow and the signature, not the protection a phone's Secure Enclave gives. `scripts/merion-approve.py` is the same signing path a phone app uses.
- Sending real mail from a demo is real: use your own address as the recipient.

## What was tested

With a real Gmail account (2026-09-27): consent, a read of unread headers, a draft that waited for the approver and landed in Drafts, a denied send that never reached Google, and the calendar agenda ([details](README.md#verified-against-real-google)).

Without an account: the script's plumbing ran end to end with a made-up client id (the runtime started, the agents deployed, the connection was stored, and Google's real token endpoint refused the made-up client with `invalid_client`, which the chat showed with the made-up token nowhere in the output), and the approval flow ran against the fake Google in `demos-ci.sh`.
