# shellcheck shell=bash
# iNilla: cross-compile the iOS build of the client on Linux, without Apple's SDK.
#
#   source ios/toolchain/linux-env.sh
#   cargo check --target aarch64-apple-ios -p inilla-ios
#   cargo build --target aarch64-apple-ios --profile ios -p inilla-ios   # → libinilla_ios.a
#
# Rust itself needs nothing from the SDK until the final link: `rustup target add
# aarch64-apple-ios` brings a prebuilt std, and a `staticlib` is an archive, not a link. The only
# thing that does need Darwin headers is the handful of C files in the graph (Lua, blake3's NEON
# path, tracing-oslog's wrapper). Those compile with the host clang against the open-source Darwin
# libc headers that Zig distributes (APSL/BSD, from Apple's open-source Libc/xnu drops), plus two
# small shims in `shims/` for <os/log.h>, which only the SDK carries.
#
# The final link (`ios/build-ipa.sh`) is the one step that runs on macOS: it needs the SDK's
# framework stubs, and that is a couple of minutes on a macOS runner instead of the full compile.

_kd_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
_kd_sysroot="${INILLA_IOS_SYSROOT:-$_kd_root/target/inilla-ios-sysroot/iPhoneOS.sdk}"

# Pinned: the headers are part of the build's inputs, so they move only with this line.
_kd_zig_ver="0.16.0"
_kd_zig_whl="ziglang-${_kd_zig_ver}-py3-none-manylinux_2_12_x86_64.manylinux2010_x86_64.musllinux_1_1_x86_64.whl"
_kd_zig_url="https://files.pythonhosted.org/packages/3e/ed/7b79023aa27ceb5d461ecf761181e7c33c57bbc1a6256a39535d1c7083d2/${_kd_zig_whl}"
_kd_zig_sha="9fcda73f62b851dd72a54b710ad40a209896db14cfb13649e62191243556342b"

_kd_make_sysroot() {
    local work="$_kd_root/target/inilla-ios-sysroot/.work"
    mkdir -p "$work"
    if [ ! -f "$work/$_kd_zig_whl" ]; then
        echo "inilla: fetching Darwin libc headers (ziglang ${_kd_zig_ver} wheel)…" >&2
        curl -fsSL -o "$work/$_kd_zig_whl.part" "$_kd_zig_url" || return 1
        mv "$work/$_kd_zig_whl.part" "$work/$_kd_zig_whl"
    fi
    echo "$_kd_zig_sha  $work/$_kd_zig_whl" | sha256sum -c --quiet - || {
        echo "inilla: ziglang wheel checksum mismatch" >&2
        rm -f "$work/$_kd_zig_whl"
        return 1
    }
    rm -rf "$work/unpacked" "$_kd_sysroot"
    python3 -m zipfile -e "$work/$_kd_zig_whl" "$work/unpacked" || return 1
    mkdir -p "$_kd_sysroot/usr"
    cp -r "$work/unpacked/ziglang/lib/libc/include/any-darwin-any" "$_kd_sysroot/usr/include"
    cp -r "$_kd_root/ios/toolchain/shims/." "$_kd_sysroot/usr/include/"
    rm -rf "$work/unpacked"
    # The Zig headers are macOS-flavoured; clang only consults this file for the SDK version.
    cat > "$_kd_sysroot/SDKSettings.json" <<'EOF'
{"CanonicalName":"iphoneos17.0","Version":"17.0","IsBaseSDK":"YES","DisplayName":"iOS 17.0 (inilla linux headers)"}
EOF
    echo "inilla: iOS sysroot ready at $_kd_sysroot" >&2
}

if [ ! -f "$_kd_sysroot/usr/include/os/log.h" ]; then
    _kd_make_sysroot || echo "inilla: sysroot setup FAILED" >&2
fi

_kd_clang="$(command -v clang-18 || command -v clang)"
_kd_ar="$(command -v llvm-ar-18 || command -v llvm-ar)"

export IPHONEOS_DEPLOYMENT_TARGET="${IPHONEOS_DEPLOYMENT_TARGET:-17.0}"
export SDKROOT="$_kd_sysroot"
for _kd_t in aarch64_apple_ios aarch64_apple_ios_sim; do
    export "CC_${_kd_t}=$_kd_clang"
    export "AR_${_kd_t}=$_kd_ar"
done
# `-isysroot` is what cc-rs derives from SDKROOT; the extra flags keep macOS-only availability
# annotations in the Zig headers from turning into errors under an iOS target.
export CFLAGS_aarch64_apple_ios="-isysroot $_kd_sysroot -Wno-unknown-warning-option -Wno-availability -Wno-nullability-completeness"
export CFLAGS_aarch64_apple_ios_sim="$CFLAGS_aarch64_apple_ios"
unset _kd_t
