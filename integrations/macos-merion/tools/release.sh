#!/usr/bin/env bash
# Build Solvor for release and package it as a .dmg.
#   tools/release.sh                       # Release build, ad-hoc signed (runs on the Mac that built it), build/Solvor-<version>.dmg
#   SIGN_IDENTITY="Developer ID Application: Name (TEAMID)" tools/release.sh --sign
#   ... --sign --notarize                  # also notarize and staple (needs the notary credentials below)
#
# Notarization credentials, one of:
#   NOTARY_PROFILE=name                    a keychain profile made once with `xcrun notarytool store-credentials`
#   NOTARY_KEY_PATH, NOTARY_KEY_ID, NOTARY_ISSUER    an App Store Connect API key (.p8), its key id and issuer id
# Nothing here uploads anything anywhere; the release workflow does that.
set -euo pipefail
cd "$(dirname "$0")/.."

SIGN=0; NOTARIZE=0
for a in "$@"; do
  case "$a" in
    --sign) SIGN=1 ;;
    --notarize) NOTARIZE=1 ;;
    -h|--help) sed -n '2,11p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown option: $a" >&2; exit 64 ;;
  esac
done
(( NOTARIZE == 0 || SIGN == 1 )) || { echo "--notarize needs --sign" >&2; exit 64; }
if (( SIGN )); then
  [[ -n "${SIGN_IDENTITY:-}" ]] || { echo "set SIGN_IDENTITY to your Developer ID Application identity" >&2; exit 64; }
fi
if (( NOTARIZE )); then
  if [[ -n "${NOTARY_PROFILE:-}" ]]; then NOTARY_ARGS=(--keychain-profile "$NOTARY_PROFILE")
  elif [[ -n "${NOTARY_KEY_PATH:-}" && -n "${NOTARY_KEY_ID:-}" && -n "${NOTARY_ISSUER:-}" ]]; then NOTARY_ARGS=(--key "$NOTARY_KEY_PATH" --key-id "$NOTARY_KEY_ID" --issuer "$NOTARY_ISSUER")
  else echo "notarizing needs NOTARY_PROFILE, or NOTARY_KEY_PATH + NOTARY_KEY_ID + NOTARY_ISSUER" >&2; exit 64; fi
fi

DD=build/Release
xcodebuild -project Solvor.xcodeproj -scheme Solvor -configuration Release -destination 'platform=macOS' -derivedDataPath "$DD" CODE_SIGNING_ALLOWED=NO build 2>&1 | tail -2
APP="$DD/Build/Products/Release/Solvor.app"
[[ -d "$APP" ]] || { echo "the build did not produce $APP" >&2; exit 1; }
VERSION="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$APP/Contents/Info.plist")"
DMG="build/Solvor-$VERSION.dmg"

if (( SIGN )); then
  # hardened runtime + a secure timestamp are required for notarization; the entitlements are the two Solvor needs
  codesign --force --options runtime --timestamp --entitlements Resources/Solvor.entitlements --sign "$SIGN_IDENTITY" "$APP"
  codesign --verify --strict --verbose=2 "$APP"
else
  # ad-hoc, but with the same entitlements and identifier as a real release, so a local dmg behaves like the signed one
  codesign --force --sign - --entitlements Resources/Solvor.entitlements "$APP"
fi

STAGE="$(mktemp -d)"; trap 'rm -rf "$STAGE"' EXIT
cp -R "$APP" "$STAGE/Solvor.app"; ln -s /Applications "$STAGE/Applications"
rm -f "$DMG"
hdiutil create -quiet -volname "Solvor $VERSION" -srcfolder "$STAGE" -ov -format UDZO "$DMG"
if (( SIGN )); then codesign --force --timestamp --sign "$SIGN_IDENTITY" "$DMG"; fi
if (( NOTARIZE )); then
  xcrun notarytool submit "$DMG" "${NOTARY_ARGS[@]}" --wait
  xcrun stapler staple "$DMG"
  xcrun stapler validate "$DMG"
fi
( cd build && shasum -a 256 "$(basename "$DMG")" > "$(basename "$DMG").sha256" )

echo
echo "built $DMG ($(du -k "$DMG" | cut -f1) KiB)"
cat "$DMG.sha256"
if (( SIGN )); then
  echo "signed as: $(codesign -dv "$APP" 2>&1 | grep -E '^Authority=' | head -1)"
  (( NOTARIZE )) && echo "notarized and stapled" || echo "NOT notarized: Gatekeeper will still warn on other Macs"
else
  echo "UNSIGNED (ad-hoc): it runs on this Mac; on another Mac Gatekeeper will refuse it. Sign and notarize with --sign --notarize."
fi
