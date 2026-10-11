# SemanticCore (Swift)

A Swift package over the `sc-ffi` C ABI. Swift tools 6.0, Swift 6 language mode, macOS 14 or later.

## Build

The package links the static library that `sc-ffi` produces. Build it first, from the repository root:

```sh
scripts/build-ffi.sh          # cargo build -p sc-ffi --release, then copies libsc_ffi.a into swift/SemanticCore/Libraries/
cd swift/SemanticCore
swift test
```

`Libraries/` and `.build/` are gitignored. The `CSemanticCore` system-library target reads the header from `crates/sc-ffi/include/sc.h` through `Sources/CSemanticCore/shim.h`; the package's linker settings add `Libraries/` to the search path and link `sc_ffi`.

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
