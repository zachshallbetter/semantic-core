# Semantic Core

A portable, local-first identity and relationship substrate for files, information artifacts, and the things they describe.

**Files have locations. Information has identity. Things have relationships.**

A receipt, a warranty, a manual and a service record can all refer to the same dishwasher while living in unrelated folders. Moving a file does not sever a justified relationship. Copying a file does not create the same instance. Identical bytes do not prove identical meaning. Semantic Core keeps those distinctions as data, with provenance and a reversible history, independently of any application, renderer or model.

## Status

Stage 1 Rust core implemented and tested: `sc-envelope`, `sc-vocab`, `sc-core` and `sc-cli` pass their fixtures under `cargo test --workspace`, and the Reflective Rust golden vector reproduces. `sc-ffi` (C ABI) and the Swift package `swift/SemanticCore` are implemented and tested: the golden vector reproduces through the FFI and independently in Swift, CryptoKit verifies the signed vector, and the Stage 1 demonstration runs through the Swift store. The Spotlight spike is not started. Nothing in this repository is a conformance claim for any external protocol.

- [Definition v0.2.2](docs/semantic-core-definition-v0.2.2.md) is the architecture of record: ontology, decision boundaries, protocol responsibilities, macOS integration, security, stage plan, fixtures, and the nine answered design decisions.
- [Roadmap](docs/ROADMAP.md) is the stage plan and task order.

## Shape

```text
crates/
  sc-envelope   sc-event-0.1 envelope: JCS canonical JSON, sha256 identity, Ed25519, access projection, freshness
  sc-vocab      sc-vocab-1 predicate registry: class constraints, inverses, unknown predicates kept as values
  sc-core       things, artifacts, revisions, instances, presentations, relationships, assertions,
                observations, hash-chained event log, SQLite materialization, resolver
  sc-cli        sc: link, move-resolve, inspect, retract, export, import, why
  sc-ffi        C ABI for the Swift package
swift/SemanticCore  Swift package over sc-ffi: Envelope, Signature, Hash, JCS, Store; golden vector both ways
fixtures/           moves, copies, edits, deletes, ambiguous, containers, access, vectors
```

## Stage 1 proof

Link two files, move one, resolve and inspect the relationship, retract it, inspect the history. Offline, with no model, no server, and no Finder extension. The full acceptance list is section 11 of the definition plus the three additions in the build brief.

## Principles that do not move

- Object identity is opaque and never derived from a path, a hash, a device or a signing key.
- Observations are not assertions. Assertions are not accepted truth. Qualification is not promotion.
- Verdicts attach to commitments, never to relationships.
- Access is enforced before traversal, counting, search, export and surface participation.
- History is append-only. Undo is a compensating event.
- The graph outlives every consumer. Finder, assistants, editors and renderers read it; none of them own it.

## License

MIT OR Apache-2.0, at your option.
