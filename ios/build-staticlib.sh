#!/usr/bin/env bash
# iNilla: build the whole client as a static library for iOS — on Linux or on macOS.
#
#   ios/build-staticlib.sh                 # → target/inilla-ios-out/libinilla_ios.a + native-libs.txt
#   INILLA_FEATURES=dev ios/build-staticlib.sh   # with benilla's dev instruments compiled in
#
# A staticlib is an archive, not a link, so no Apple linker or SDK is involved: on Linux the few C
# files in the graph compile against open-source Darwin headers (ios/toolchain/linux-env.sh).
# The link into an app is ios/package-ipa.sh, on macOS.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

PROFILE="${INILLA_PROFILE:-ios}"
TARGET="aarch64-apple-ios"
FEATURES="${INILLA_FEATURES:-}"
OUT="$ROOT/target/inilla-ios-out"
export IPHONEOS_DEPLOYMENT_TARGET="${IPHONEOS_DEPLOYMENT_TARGET:-17.0}"

if [ "$(uname -s)" != "Darwin" ]; then
    # shellcheck source=toolchain/linux-env.sh
    source "$ROOT/ios/toolchain/linux-env.sh"
fi

rustup target add "$TARGET" >/dev/null 2>&1 || true

feature_args=()
if [ -n "$FEATURES" ]; then
    feature_args=(--features "$FEATURES")
fi

echo "inilla: building $TARGET ($PROFILE${FEATURES:+, features: $FEATURES})"
mkdir -p "$OUT"
messages="$OUT/build-messages.json"
cargo build --target "$TARGET" --profile "$PROFILE" -p inilla-ios "${feature_args[@]}" \
    --message-format=json-render-diagnostics >"$messages"

# A native library a build script links as `static:-bundle` (Lua 5.1, from third_party/lua-src)
# is not inside the staticlib: ship each one beside it, in lib/, for the link to find.
rm -rf "$OUT/lib"
mkdir -p "$OUT/lib"
jq -r 'select(.reason == "build-script-executed")
       | . as $m
       | .linked_libs[]
       | select(test("^static:([^=]*,)?-bundle"))
       | sub("^[^=]*="; "") as $name
       | $m.linked_paths[] | sub("^[a-z]*="; "") | "\(.)/lib\($name).a"' "$messages" |
    while read -r archive; do
        if [ -f "$archive" ]; then
            cp "$archive" "$OUT/lib/"
            echo "inilla: unbundled archive $(basename "$archive")"
        fi
    done

# The system libraries and frameworks the archive needs at link time, as rustc reports them. Only
# the tiny inilla-ios crate is re-run for this.
notes="$OUT/native-static-libs.log"
cargo rustc --color never --target "$TARGET" --profile "$PROFILE" -p inilla-ios --lib \
    "${feature_args[@]}" -- --print native-static-libs 2>"$notes" || { cat "$notes"; exit 1; }
# Colour must not reach the flags (CI sets CARGO_TERM_COLOR=always): `-lm` + ESC[0m is no library.
esc=$'\033'
libs="$(sed -e "s/${esc}\[[0-9;]*m//g" "$notes" | sed -n 's/.*native-static-libs: //p' | tail -n 1)"
if [ -z "$libs" ]; then
    echo "inilla: rustc printed no native-static-libs line:" >&2
    cat "$notes" >&2
    exit 1
fi
if printf '%s' "$libs" | LC_ALL=C grep -q '[^[:print:]]'; then
    echo "inilla: control characters in the link flags: $(printf '%s' "$libs" | cat -v)" >&2
    exit 1
fi
echo "$libs" >"$OUT/native-libs.txt"

cp "target/$TARGET/$PROFILE/libinilla_ios.a" "$OUT/libinilla_ios.a"
git -C "$ROOT" rev-parse --short HEAD >"$OUT/commit.txt" 2>/dev/null || echo unknown >"$OUT/commit.txt"
ls -lh "$OUT/libinilla_ios.a"
echo "inilla: link flags: $libs"
