#!/bin/sh
# Build a macOS universal static XCFramework for the Swift package.
# Requires Xcode and Rust targets aarch64-apple-darwin,x86_64-apple-darwin.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
profile=release
flag=--release
case "${1:-}" in
    '') ;;
    --debug) profile=debug; flag='' ;;
    *) echo 'usage: build-ffi.sh [--debug]' >&2; exit 2 ;;
esac
[ "$(uname -s)" = Darwin ] || { echo 'XCFramework builds require macOS and Xcode' >&2; exit 1; }
cd "$root"
export MACOSX_DEPLOYMENT_TARGET=14.0
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT HUP INT TERM
for target in aarch64-apple-darwin x86_64-apple-darwin; do
    # shellcheck disable=SC2086
    cargo build -p sc-ffi --locked --target "$target" $flag
done
mkdir -p "$work/headers"
cp crates/sc-ffi/include/sc.h "$work/headers/sc.h"
cat > "$work/headers/module.modulemap" <<'MAP'
module CSemanticCore {
    header "sc.h"
    export *
}
MAP
xcrun lipo -create \
    target/aarch64-apple-darwin/"$profile"/libsc_ffi.a \
    target/x86_64-apple-darwin/"$profile"/libsc_ffi.a \
    -output "$work/libsc_ffi.a"
xcodebuild -create-xcframework -library "$work/libsc_ffi.a" \
    -headers "$work/headers" -output "$work/CSemanticCore.xcframework"
mkdir -p swift/SemanticCore/Libraries
rm -rf swift/SemanticCore/Libraries/CSemanticCore.xcframework
mv "$work/CSemanticCore.xcframework" swift/SemanticCore/Libraries/
echo "swift/SemanticCore/Libraries/CSemanticCore.xcframework ($profile, macOS arm64 + x86_64)"
