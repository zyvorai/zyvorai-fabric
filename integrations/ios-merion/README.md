# Merion for iPhone

A SwiftUI app for a person's own Merion host: chat with an agent, and approve or deny what it wants to do, signed on the phone.

- **Chats**: the conversations the host holds for you (`/v1/threads`), and a chat that streams the agent's answer over AG-UI (`/v1/agui`). When the agent is waiting for a person (say, to send mail), the chat shows a read-only card with **what the host read out of the request** (recipients, subject, text; see [connectors](../../docs/merion/connectors/README.md#what-the-person-sees-before-they-approve)). A chat can never approve or deny anything.
- **Approvals**: what is waiting for you, with the same preview and the exact text this device signs (`merion-approval-v1`, [the phone side](../../docs/merion/mobile/README.md)). Approve or deny asks for Face ID or Touch ID: the key is a P-256 key in the Secure Enclave and never leaves it. The signature covers the digest of the preview, so the host cannot show one thing and have you sign another.
- **Goals**: your goals with progress, a plan an agent proposed (its steps, with Accept, Accept and run, and Discard; a plan from a planner that had read untrusted content asks you to confirm), "Ask the agent for a plan" for a goal with no steps, pause and cancel, and the suggestions agents made (a switch, off by default, then "Make it a goal" or Dismiss). Accepting starts nothing by itself; a step that sends or spends still waits in Approvals.
- **You**: your **memory** (off by default; add a note, accept or refuse what an agent suggested remembering, delete entries, forget everything) and **Done**, a read-only list of what happened after you approved things (method, host and path, how it ended, the agent; never the message).
- **Settings**: the host address and your user token (the token is kept in the Keychain and sent only to that host, in a header), the agent to chat with, and this device's public key with the details an operator needs to enrol it (`POST /v1/users/<you>/devices`; a user token is deliberately not allowed to add its own key).

It reuses `MerionKit` from [`../macos-merion`](../macos-merion/) (the API client, the signer, the AG-UI stream), so the Mac and the phone share one tested implementation; `MerionKit` now also builds for iOS.

## Build

```bash
brew install xcodegen
cd integrations/ios-merion && xcodegen generate
xcodebuild -project MerionPhone.xcodeproj -scheme MerionPhone -destination 'generic/platform=iOS Simulator' CODE_SIGNING_ALLOWED=NO build
```

`MerionKit`'s tests (including the chat stream, the preview, goals, plans, suggestions, memory and receipts) run with `swift test` in `../macos-merion`.

## What is and is not verified

- Verified: `MerionKit` builds for iOS and its tests pass on the Mac (event mapping, the chat request and stream against a stub, thread listing, preview decoding); the app compiles for the iOS simulator SDK.
- **Not verified: the app has not been run**, not in a simulator and not on a phone. The screens are unexercised, and the Secure Enclave signing path is unexercised on iOS (the simulator has no Secure Enclave; only a real device does).
- **Needs the owner:** an Apple developer team to sign it and run it on a device or TestFlight (`DEVELOPMENT_TEAM` in Xcode; nothing here holds a signing secret), and push notifications (APNs) need an Apple key, so today the Approvals tab is refreshed by pulling, not pushed.
- Not there yet: a QR/link based way to enrol, push, Android.
