#!/bin/sh
# Assemble a self-contained package, without the source repository or Rust.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
artifact=swift/SemanticCore/Libraries/CSemanticCore.xcframework
[ -d "$artifact" ] || { echo 'Run scripts/build-ffi.sh first' >&2; exit 1; }
[ ! -e dist/SemanticCore ] || { echo 'dist/SemanticCore already exists; move it before rebuilding' >&2; exit 1; }
mkdir -p dist
work=$(mktemp -d "$root/dist/.swift-package.XXXXXX")
trap 'rm -rf "$work"' EXIT HUP INT TERM
mkdir -p "$work/Sources" "$work/Libraries"
# Include only shipping targets. Development tests refer to repository fixtures.
sed '/        .testTarget(/,/        ),/d' swift/SemanticCore/Package.swift > "$work/Package.swift"
cp -R swift/SemanticCore/Sources/SemanticCore "$work/Sources/"
cp -R swift/SemanticCore/Sources/SemanticCoreSpotlight "$work/Sources/"
cp -R "$artifact" "$work/Libraries/"
cp swift/SemanticCore/README.md "$work/README.md"
cp LICENSE LICENSE-MIT LICENSE-APACHE "$work/"
git rev-parse HEAD > "$work/SOURCE_COMMIT"
mv "$work" dist/SemanticCore
echo 'dist/SemanticCore: standalone macOS SwiftPM package'
