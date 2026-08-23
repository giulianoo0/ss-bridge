#!/usr/bin/env bash
set -euo pipefail

APP="dist/ss-bridge.app"
rm -rf "$APP" dist/ss-bridge-macos.dmg
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp packaging/Info.plist "$APP/Contents/Info.plist"
cp packaging/AppIcon.icns "$APP/Contents/Resources/AppIcon.icns"
cp dist/ss-bridge "$APP/Contents/MacOS/ss-bridge"
chmod +x "$APP/Contents/MacOS/ss-bridge"

if [ -n "${CERT_B64:-}" ] && [ -n "${SIGN_ID:-}" ]; then
  KEYCHAIN="$RUNNER_TEMP/build.keychain"
  KEYCHAIN_PWD="ci-$RANDOM"
  security create-keychain -p "$KEYCHAIN_PWD" "$KEYCHAIN"
  security set-keychain-settings -lut 21600 "$KEYCHAIN"
  security unlock-keychain -p "$KEYCHAIN_PWD" "$KEYCHAIN"
  security list-keychains -d user -s "$KEYCHAIN" $(security list-keychains -d user | tr -d '"')
  echo "$CERT_B64" | base64 --decode > "$RUNNER_TEMP/cert.p12"
  security import "$RUNNER_TEMP/cert.p12" -k "$KEYCHAIN" -P "${CERT_PWD:-}" -T /usr/bin/codesign
  security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$KEYCHAIN_PWD" "$KEYCHAIN" >/dev/null
  codesign --force --deep --timestamp --options runtime \
    --entitlements packaging/entitlements.plist --sign "$SIGN_ID" "$APP"
  codesign --verify --deep --strict --verbose=2 "$APP"
else
  echo "No signing secrets set; producing an unsigned .app"
fi

create-dmg \
  --volname "ss-bridge" \
  --background packaging/dmg-background.png \
  --window-pos 200 120 \
  --window-size 540 380 \
  --icon-size 120 \
  --icon "ss-bridge.app" 140 190 \
  --app-drop-link 400 190 \
  --no-internet-enable \
  dist/ss-bridge-macos.dmg "$APP"

if [ -n "${AC_ID:-}" ] && [ -n "${AC_PWD:-}" ] && [ -n "${AC_TEAM:-}" ]; then
  xcrun notarytool submit dist/ss-bridge-macos.dmg \
    --apple-id "$AC_ID" --password "$AC_PWD" --team-id "$AC_TEAM" --wait
  xcrun stapler staple dist/ss-bridge-macos.dmg
fi

rm -rf "$APP"
