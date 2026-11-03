#!/bin/sh
# Exercise SwiftPM Git dependency rules rather than a permissive path dependency.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT HUP INT TERM
cp -R "$root/dist/SemanticCore" "$work/SemanticCore"
cd "$work/SemanticCore"
git init -q
git add .
git -c user.name='Distribution Test' -c user.email='test@example.invalid' commit -qm 'Test distribution'
git tag 0.0.1
mkdir -p "$work/Consumer/Sources/Consumer"
cd "$work/Consumer"
cat > Package.swift <<MANIFEST
// swift-tools-version: 6.0
import PackageDescription
let package = Package(
    name: "Consumer",
    platforms: [.macOS(.v14)],
    dependencies: [.package(url: "file://$work/SemanticCore", exact: "0.0.1")],
    targets: [.executableTarget(name: "Consumer", dependencies: [
        .product(name: "SemanticCore", package: "SemanticCore"),
        .product(name: "SemanticCoreSpotlight", package: "SemanticCore")
    ])]
)
MANIFEST
cat > Sources/Consumer/main.swift <<'SWIFT'
import Foundation
import SemanticCore
import SemanticCoreSpotlight

let json = #"{"schema_version":"sc-event-0.1","subject_id":"sc:thing:01J9ZX0000000000000000AVA1","compatibility":{"protocol":"sc-event-0.1","subject_schema":"note-1"},"provenance":{"kind":"declared","authority":"owner-local","source":"actor:ava","source_hash":"sha256:6186c1f131e8c42ed706e0180641b2c414f1a34ae8ca4ed98884c140768cfd2a"},"minimum_access":"owner","body":{"text":"Ava's dishwasher","members":[{"name":"title","minimum_access":"public"},{"name":"serial","minimum_access":"owner"}]}}"#
let canonical = try JCS.canonicalize(json)
let ffiCanonical = try Envelope.canonicalJSON(of: json)
let ffiHash = try Envelope.hash(of: json)
precondition(ffiCanonical == canonical)
precondition(ffiHash == Hash.sha256(canonical))
let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
defer { try? FileManager.default.removeItem(at: directory) }
let store = try Store(path: directory.appendingPathComponent("test.sqlite").path, owner: "consumer")
let id = try store.addThing(label: "Distribution test")
let inspection = try store.inspect(id)
precondition(inspection["kind"]?.string == "thing")
store.close()
print("Git dependency consumer: canonicalization, hashing, Store and Spotlight import passed")
SWIFT
swift run Consumer
