# Releasing Solvor

A release is a tag. The `Release` workflow builds the app, packages `Solvor-<version>.dmg`, and publishes it. **Without the Apple secrets below it builds an
unsigned dmg and publishes a pre-release marked UNSIGNED**: nothing is signed or notarized until you add them. Nothing here has been run with real Apple credentials:
the unsigned path is tested (below); the signing and notarization steps follow Apple's documented `codesign` / `notarytool` / `stapler` flow and are **unverified until the first signed release**.

## What you need (once, from the Apple Developer account)

1. A **Developer ID Application** certificate. Export it with its private key as a `.p12` from Keychain Access.
2. An **App Store Connect API key** (Users and Access, Integrations, Keys) for notarization: the `.p8` file, its key id and the issuer id.

Add these repository secrets (Settings, Secrets and variables, Actions). **Never paste them anywhere else, and never give them to a tool or an assistant.**

| Secret | Value |
|---|---|
| `MACOS_CERT_P12_BASE64` | `base64 -i DeveloperID.p12` |
| `MACOS_CERT_PASSWORD` | the `.p12` password |
| `MACOS_SIGN_IDENTITY` | `Developer ID Application: Your Name (TEAMID)` |
| `APPSTORE_KEY_P8_BASE64` | `base64 -i AuthKey_XXXX.p8` |
| `APPSTORE_KEY_ID` | the key id |
| `APPSTORE_ISSUER_ID` | the issuer id |

## Cut a release

```bash
# bump MARKETING_VERSION in project.yml, then: make project (xcodegen) and commit
git tag v0.1.0 && git push --tags       # the Release workflow builds, signs, notarizes, staples and publishes
```

A run started by hand (Actions, Release, Run workflow) builds the dmg and uploads it as an artifact without publishing a release.

## Build locally

```bash
make dmg                                # Release build, ad-hoc signed: runs on this Mac only. Prints the sha256.
make release SIGN_IDENTITY="Developer ID Application: Your Name (TEAMID)" NOTARY_PROFILE=name   # signed and notarized (needs the credentials)
```

`tools/release.sh` applies `Resources/Solvor.entitlements` (Apple Events, to read a browser tab; the microphone, for Talk to Solvor) with the hardened runtime when signing.

## Homebrew

`docs/dist/solvor.rb` is a cask template. After a **signed** release, copy it to `Casks/solvor.rb` in a tap repository (for example `zyvorai/homebrew-tap`), set `version` and the `sha256` from the release's
`.sha256` file, and users install with `brew install --cask zyvorai/tap/solvor`. Do not publish the cask for an unsigned release: Gatekeeper refuses it after install.

## What is verified

- `make dmg` builds a Release app and a dmg on macOS 26: the dmg mounts, holds `Solvor.app` and an `/Applications` link, the app carries the bundle identifier `dev.zyvor.solvor`, the Apple Events and microphone entitlements, and launches from the mounted image.
- The workflow file parses; the signing steps have not run.
- **Not verified:** signing with a real certificate, notarization, stapling, `spctl` acceptance on another Mac, and the cask.
