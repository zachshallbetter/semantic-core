# SemanticCore (Swift)

A Swift package over the `sc-ffi` C ABI. Swift tools 6.0, Swift 6 language mode, macOS 14 or later.

## Build

Source checkouts build the Rust FFI into a universal macOS XCFramework. Install Xcode and both Rust targets, then build from the repository root:

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
scripts/build-ffi.sh          # locked release builds, universal macOS XCFramework
cd swift/SemanticCore
swift test
```

`Libraries/` and `.build/` are gitignored. The `CSemanticCore` binary target includes its own C header and module map; no header reaches outside the package and no target uses `unsafeFlags`. The XCFramework contains arm64 and x86_64 macOS slices with a macOS 14 deployment target. `--debug` builds both slices in debug mode.

## Distribution

After building the XCFramework, run `scripts/package-swift.sh` to assemble `dist/SemanticCore`. It includes the Swift sources, the binary target, licenses, and source commit identifier. It needs neither the Rust workspace nor a consumer-side Cargo build. The packaging script refuses to overwrite an existing distribution.

CI tests the full source package, then assembles this distribution and runs `scripts/test-swift-distribution.sh`. That test commits the complete distribution to a temporary Git repository and consumes it by URL and version from a separate Swift package. It imports both products and exercises FFI canonicalization, hashing and SQLite-backed Store operations. CI uploads the self-contained package as `SemanticCore-macOS-SwiftPM`.

For a remote SwiftPM dependency, publish the complete distribution directory, including `Libraries/CSemanticCore.xcframework`, to a tagged package repository. Point the consumer at that repository. The source repository alone is not a ready-to-consume remote dependency because its generated binaries are gitignored. No release or distribution repository is published automatically. Only macOS is supported; iOS and other Apple platforms require additional native builds and platform declarations.

## Implemented

- `Envelope`: canonical bytes, canonical hash and validation through the core.
- `Signature`: sign with a 32-byte seed, verify with a 32-byte public key through the core; the domain-separated payload and base64url decoding for independent checks.
- `Hash`: `sha256:<hex>` over bytes (CryptoKit).
- `JCS`: an independent RFC 8785 canonicalizer for the envelope's value space (object members sorted by UTF-16 code units, no whitespace, strings escaped per the RFC, integers only and within 2^53). Fractions and exponents are refused.
- `Store`: open or create, add thing, add file, link, resolve, gone, inspect, why, history, retract, set audience, acting as the library owner; `inspect(_:as:)` reads as another `Principal` under the core's access rule. Records come back as decoded summaries or as `JSONObject`.
- `SemanticCoreSpotlight` (separate product): `ThingEntity: AppEntity, IndexedEntity` with a Core Spotlight `attributeSet` built only from a viewer-projected `inspect` (title, display name, identifier, content type, present relationships, keywords from registered predicates), and `ThingQuery` resolving through an installed provider. Everything in it is `@available(macOS 15, *)`; the package still builds for macOS 14.
- `SemanticCoreError`: the core's code and last-error message.

## Tested (`swift test`)

- The Reflective Rust reference vector hashes to `sha256:7c98ac356aa59dd29e466c6f6e5927eb5afd2ad85f283d59d920de3f7bf05a18` through the FFI and, independently, through `JCS` and `Hash`.
- The three `sc-event-0.1` vectors reproduce their canonical bytes and hashes both ways; CryptoKit's Curve25519 verifies the signed vector over the same domain-separated payload, and re-signing with the published seed yields the same bytes.
- The tampered vector fails verification in both paths; the rotated key refuses the old signature.
- The Stage 1 demonstration through `Store`: link two files, move one, resolve, inspect, retract, inspect the history.
- Spotlight spike (`SemanticCoreSpotlightTests`): the owner's entity names the thing and its relationships; a restricted viewer's entity, serialized with `NSKeyedArchiver`, carries no identifier, label, path, predicate, audience or count from an artifact outside their audience; a thing the viewer cannot see produces no entity, exactly like an absent identifier.

## Not proven

No live Spotlight query is exercised. Registering an `IndexedEntity` with `CSSearchableIndex` needs an app bundle and entitlements, so the attribute set is tested as data and real indexing is Stage 2. `hideInSpotlight` (macOS 15.4) is not used.

## Not implemented

Mutation as principals other than the owner, export and import, presentations, qualification, Finder integration. `Store` is not thread-safe.
