#!/bin/sh
# Build the sc-ffi static library and place it where the Swift package links
# it. Run from anywhere; paths are resolved from this script.
#
#   scripts/build-ffi.sh            release build (default)
#   scripts/build-ffi.sh --debug    debug build
#
# Output: swift/SemanticCore/Libraries/libsc_ffi.a (gitignored).
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
profile=release
flag=--release
if [ "${1:-}" = "--debug" ]; then
    profile=debug
    flag=""
fi
cd "$root"
# Match the Swift package's deployment target so the linker does not warn.
export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-14.0}"
# shellcheck disable=SC2086
cargo build -p sc-ffi $flag
mkdir -p swift/SemanticCore/Libraries
cp "target/$profile/libsc_ffi.a" swift/SemanticCore/Libraries/libsc_ffi.a
echo "swift/SemanticCore/Libraries/libsc_ffi.a ($profile)"
