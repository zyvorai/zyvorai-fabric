# A Windows companion (and anyone who is not on an Apple phone)

## Why

Keep's clients are a Mac app (Solvor), an iPhone app and a chat page. A person on Windows or Linux can use the chat page, but cannot
**approve** anything there: approvals are signed with a key held in a phone's Secure Enclave or a Mac's, and the chat page deliberately
cannot decide them. So today a Windows-only person cannot use the part of Keep that matters most.

## What exists

- The signed decision (`keep-approval-v1`): a P-256 or Ed25519 signature over a readable payload that includes the approval id, the decision, the
  action's digest (which covers the host-rendered preview), a challenge and an expiry. Enrolled per user by the operator
  ([mobile/README.md](../mobile/README.md)); verified in `devices.rs`.
- Signers today: Secure Enclave (Mac, iPhone), a software key file (tests and the terminal approver), a sketch for Android StrongBox.
- The chat page, a local proxy for one person on one machine.

## Proposal: two separate things

**1. Approvals from any operating system, with no native app, through WebAuthn.** WebAuthn (passkeys and platform authenticators such as
Windows Hello, a phone, or a security key) signs a challenge with a key that never leaves the authenticator and requires the person's
presence and, if the authenticator supports it, verification (a fingerprint, face or PIN). Add a second enrolment and verification path
in the runtime:
- **Enrol:** a WebAuthn credential (ES256 public key, credential id) registered for the user, bound to the host's relying-party id, done by
  the operator or the gateway after a strong login, exactly like a device key today.
- **Challenge:** the approval's challenge is the SHA-256 of the same `keep-approval-v1` payload, so the authenticator signs over what is
  displayed; the verification checks the assertion per the spec (the relying-party id hash, the presence and verification flags, the
  challenge in `clientDataJSON`, the origin, the signature over `authenticatorData || SHA-256(clientDataJSON)`), and a signature counter that does not go back.
- **Where the page runs:** a small approvals page served by the host (or the chat page's proxy) that shows the preview and calls
  `navigator.credentials.get`. It still **cannot decide anything without the authenticator**.
This gives Windows, Linux and Android a signing path with hardware protection when the authenticator has it, and no installer.

**2. A Windows tray app only for what a browser cannot do:** watched folders (the same idea as Solvor's), a drop zone and a "send to Keep" verb.
Not before (1) exists and someone asks for it, because (1) is what unblocks Windows people, and (2) is convenience.

## What it must never do

- Read files, the clipboard or the screen except on the person's explicit action.
- Automate the Windows desktop for an agent (an agent that operates a Windows session is a separate project, listed as not planned in
  [ROADMAP.md](../ROADMAP.md)).
- Hold a signing key in a file the person's other software can read (a software key is for tests only).
- Decide an approval from a notification button.

## How I would verify it

A software WebAuthn authenticator in tests (the runtime's verification checked against known test vectors from the specification, then a
generated authenticator); a wrong relying-party id, a missing user-presence flag, a replayed or older counter, a flipped decision and a
tampered `clientDataJSON` are each refused; enrolment needs the operator, and a user token cannot enrol its own credential. Then a real run
by you with Windows Hello (or any platform authenticator) and, separately, a phone as the authenticator.

## Size

WebAuthn verification, enrolment and the approvals page: medium to large (careful spec work; the crypto is small, the edge cases are not).
The tray app: medium and separate. Two or three PRs for the first.

## Decisions for you

1. **Is Windows a real audience right now?** If not, this waits. If yes: recommendation, WebAuthn first.
2. **Which authenticators must work?** Windows Hello only, or also a phone and hardware keys? Recommendation: all three; the protocol is the same.
3. **Where is the relying-party id?** A WebAuthn credential is bound to a domain. That means a stable host name for the approvals page
   (a local demo on `localhost` works; a real deployment needs its own name). What name will it be?
4. **A tray app later?** Not now, unless you know of a person who needs watched folders on Windows.
