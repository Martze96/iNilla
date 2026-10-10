#!/usr/bin/env bash
# iNilla: link the static library into iNilla.app and package iNilla.ipa — the one step that
# needs macOS (Apple's linker and the iPhoneOS SDK's framework stubs). A couple of minutes.
#
#   ios/package-ipa.sh      # reads target/inilla-ios-out/{libinilla_ios.a,lib/,native-libs.txt}
#                           # writes target/inilla-ios-out/iNilla.ipa
#
# Knobs: INILLA_BUNDLE_ID (default org.inilla.app), INILLA_VERSION, INILLA_BUILD,
# INILLA_CLAIM_SDK (see below), INILLA_STRIP=1 (drop local symbols; smaller, but crash
# backtraces lose function names).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="$ROOT/target/inilla-ios-out"
MIN_OS="${IPHONEOS_DEPLOYMENT_TARGET:-17.0}"
BUNDLE_ID="${INILLA_BUNDLE_ID:-org.inilla.app}"
VERSION="${INILLA_VERSION:-0.1.0}"
BUILD="${INILLA_BUILD:-$(date -u +%Y%m%d%H%M)}"

[ "$(uname -s)" = "Darwin" ] || { echo "package-ipa.sh runs on macOS (it needs Apple's linker)"; exit 1; }
[ -f "$OUT/libinilla_ios.a" ] || { echo "no $OUT/libinilla_ios.a — run ios/build-staticlib.sh first"; exit 1; }

SDK="$(xcrun --sdk iphoneos --show-sdk-path)"
SDK_VERSION="$(xcrun --sdk iphoneos --show-sdk-version)"
SDK_MAJOR="${SDK_VERSION%%.*}"
echo "inilla: iPhoneOS SDK $SDK_VERSION ($(xcodebuild -version | head -n1))"

# **The SDK version the binary claims.** iOS keys behaviour changes on the SDK an app was linked
# against, and Apple has said that from the release after iOS 26 an app linked against the newest
# SDK must adopt the UIScene life cycle or it will not launch. winit 0.30 (under Bevy 0.18) drives
# UIKit through the classic app delegate, so the binary claims at most SDK 26.0 unless told
# otherwise — the same result as building with Xcode 26, whatever Xcode this runner has.
if [ -z "${INILLA_CLAIM_SDK:-}" ]; then
    if [ "$SDK_MAJOR" -ge 27 ]; then INILLA_CLAIM_SDK="26.0"; else INILLA_CLAIM_SDK="$SDK_VERSION"; fi
fi

APP="$OUT/Payload/iNilla.app"
rm -rf "$OUT/Payload" "$OUT/iNilla.ipa"
mkdir -p "$APP"

CC=(xcrun --sdk iphoneos clang -arch arm64 "-mios-version-min=$MIN_OS" -isysroot "$SDK")
"${CC[@]}" -c "$ROOT/ios/main.c" -o "$OUT/main.o"

# shellcheck disable=SC2046 # the flags are a list, split on purpose
"${CC[@]}" "$OUT/main.o" "$OUT/libinilla_ios.a" -L"$OUT/lib" $(cat "$OUT/native-libs.txt") \
    -Wl,-dead_strip -o "$APP/iNilla"

if [ "$INILLA_CLAIM_SDK" != "$SDK_VERSION" ]; then
    echo "inilla: rewriting LC_BUILD_VERSION: ios $MIN_OS, sdk $INILLA_CLAIM_SDK (linked with $SDK_VERSION)"
    xcrun vtool -set-build-version ios "$MIN_OS" "$INILLA_CLAIM_SDK" -replace -output "$APP/iNilla.tmp" "$APP/iNilla"
    mv "$APP/iNilla.tmp" "$APP/iNilla"
fi
if [ "${INILLA_STRIP:-0}" = "1" ]; then
    xcrun strip -x "$APP/iNilla"
fi

sed -e "s/@BUNDLE_ID@/$BUNDLE_ID/" -e "s/@VERSION@/$VERSION/" -e "s/@BUILD@/$BUILD/" \
    -e "s/@SDK_VERSION@/$INILLA_CLAIM_SDK/g" -e "s/@MIN_OS@/$MIN_OS/" \
    "$ROOT/ios/Info.plist.in" >"$APP/Info.plist"
plutil -lint "$APP/Info.plist"
plutil -convert binary1 "$APP/Info.plist"
printf 'APPL????' >"$APP/PkgInfo"
cp "$ROOT"/ios/icons/AppIcon60x60@2x.png "$ROOT"/ios/icons/AppIcon60x60@3x.png \
   "$ROOT"/ios/icons/AppIcon76x76@2x.png "$ROOT"/ios/icons/AppIcon83.5x83.5@2x.png "$APP/"

# Ad-hoc signature: SideStore / AltStore / LiveContainer re-sign with the player's own identity.
codesign --force --sign - --entitlements "$ROOT/ios/iNilla.entitlements" --timestamp=none "$APP"
codesign --verify --verbose=2 "$APP"

(cd "$OUT" && zip -qry iNilla.ipa Payload)
ls -lh "$OUT/iNilla.ipa" "$APP/iNilla"
xcrun vtool -show-build "$APP/iNilla" | sed -n '1,12p'
echo "inilla: $OUT/iNilla.ipa"
