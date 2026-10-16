# Verify the parts of Solvor that need you

These paths are built and unit-tested, but they need a permission, a device or a voice that only you can give, so they are still marked
**not verified** in the [README](../README.md). Each item says what to do, what a pass looks like and what to send back. Mark an item
verified in the README only after it passes on your Mac. Use a **user token** for a Merion host you run, never the operator token.

Setup once: `make run KEEP_HOST=http://127.0.0.1:9096 KEEP_TOKEN=kut1.…` (a debug build; no Keychain prompt) or connect in Settings.

## 1. Read an email from the browser

**Step A, a local test page first (no mailbox involved).**
1. `make verify-page` serves `docs/verify/webmail-test.html` on `http://127.0.0.1:8765/webmail-test.html`. Open that URL in Safari or Chrome.
2. In the browser, allow scripting from Apple Events, once:
   - Safari: Settings, Advanced, tick "Show features for web developers", then Develop menu, "Allow JavaScript from Apple Events".
   - Chrome, Brave, Edge or Arc: View, Developer, "Allow JavaScript from Apple Events".
3. In Solvor click **Read email from browser** (the envelope button). macOS asks whether Solvor may control the browser: click **OK**.

**Pass:** the preview shows the host `127.0.0.1`, the subject "Reminder: INV-2026-0142 is overdue" and the message text. "Codes (1)", "Long numbers (1)" and
"Tracking links (1)" are ticked, the code `482913` and account number are hidden in the text that will be sent, and *Receivables from mail*
is pre-selected. **Send to a sealed cell** returns a result with "0 outbound connections".

**Step B, one real webmail message.** Open a message you are happy to send to your own Merion host, select just the message text (the app then
reads only the selection), and repeat. Try each browser you use.

**Send back:** pass or fail per browser, and for a fail the message the sheet shows. If it says the scripting switch is off, redo step 2.

## 2. Siri and Shortcuts

1. Run the app once (`make run`), then in Terminal: `shortcuts list | grep -i solvor`.
2. Open the Shortcuts app, search "Solvor", confirm the actions appear: read the email in my browser, summarise my latest download, show approvals.
3. Say "Hey Siri, show my approvals in Solvor", then "read my email with Solvor" with the test page open.

**Pass:** the actions are listed, Siri opens Solvor on the right screen, and the email action opens the same preview sheet as the button.
**Must not happen:** Siri approving, denying, sending or deleting anything. There is no such action; report it if you find one.

## 3. Talk to Solvor (microphone, Speech, Translation)

1. Open **Talk to Solvor** (microphone button). Allow Microphone and Speech Recognition when macOS asks.
2. Pick a non-English language, say "show my approvals" in it. The first use may ask to download the Translation language: allow it.
3. Also type a command in the box, for example `summarise my latest download`.

**Pass:** it shows what it heard and the English it will act on, waits for **Do it**, then acts. Nothing runs without the click.
**Send back:** the languages you tried, and any permission message the sheet shows.

## 4. Services, menu bar, keep:// link

- **Services:** in Finder, right-click a small text file, Services, **Send to Merion**. Pass: Solvor opens and runs it (first run may need Finder
  Services enabled in System Settings, Keyboard, Keyboard Shortcuts, Services).
- **Menu bar:** the Solvor glyph appears; type a question in the ask box and press Return, it should open the main window on Agent Home with that
  message already sent; drop a file on the dashed target, it routes the same way a window drop does; recent runs and waiting approvals are listed.
- **URL scheme:** `open "keep://run?usecase=csv-clean&path=$HOME/Desktop/test.csv"` with a small CSV there. LaunchServices only binds the scheme for an
  app in `/Applications`, so copy the app there first (`cp -R build/DerivedData/Build/Products/Debug/Solvor.app /Applications/`).

## 5. The ⌥Space quick-ask hotkey

1. With Solvor running (windowed or just the menu-bar item), press **⌥Space** from any other app.
2. A small panel should appear near the top of the screen with a text field. Type something and press Return.
3. Press **⌥Space** again while the panel is open: it should close (a toggle, not a second panel).

**Pass:** the panel appears without macOS asking for Accessibility permission (it uses local/global `NSEvent` monitors, not a system-wide event
tap), and the message you typed shows up in Agent Home (Solvor's main window) as if you had typed it there.
**Send back:** whether ⌥Space conflicts with another app's shortcut on your Mac (Option-Space is also a common "next input source" shortcut in
some layouts) — if it does, that is worth a follow-up to make the hotkey configurable.

## 6. Approving a waiting request (Touch ID)

This needs a request waiting for you and a device enrolled in the Secure Enclave, which needs an operator token once. It is the longest check, so do it
with me: I set up the waiting approval on the lab host with `scripts/merion-live-tenancy.sh` (needs `KEEP_POLICY_SEED`) and you approve it in
**Approvals** with Touch ID.

**Pass:** the approval shows the action text, Touch ID approves it, the host accepts the signature, and the agent continues. Denying works the same way.

## When all six pass
Move the item from "Built but not verified" to "Verified" in [README.md](../README.md) and in `docs/merion/MACOS-APP.md` (in the fabric repo), with the date and macOS version.
