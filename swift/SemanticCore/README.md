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
- `Store`: open or create, add thing, add file, link, resolve, gone, inspect, why, history, retract, acting as the library owner. Records come back as decoded summaries or as `JSONObject`.
- `SemanticCoreError`: the core's code and last-error message.

## Tested (`swift test`)

- The Reflective Rust reference vector hashes to `sha256:7c98ac356aa59dd29e466c6f6e5927eb5afd2ad85f283d59d920de3f7bf05a18` through the FFI and, independently, through `JCS` and `Hash`.
- The three `sc-event-0.1` vectors reproduce their canonical bytes and hashes both ways; CryptoKit's Curve25519 verifies the signed vector over the same domain-separated payload, and re-signing with the published seed yields the same bytes.
- The tampered vector fails verification in both paths; the rotated key refuses the old signature.
- The Stage 1 demonstration through `Store`: link two files, move one, resolve, inspect, retract, inspect the history.

## Not implemented

Principals other than the owner, export and import, presentations, qualification, Finder or Spotlight integration. `Store` is not thread-safe.
