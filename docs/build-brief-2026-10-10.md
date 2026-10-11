# Semantic Core — build brief

**Date:** 2026-10-10
**Status:** Started 2026-10-10. Repository: https://github.com/zachshallbetter/semantic-core (public). Task 1 done on main; tasks 2 to 7 in progress on branch `stage1-rust-core` from a cloud session. Everything below was answered by Zach on 2026-10-10.
**Definition of record:** [semantic-core-definition-v0.2.2.md](semantic-core-definition-v0.2.2.md)
**History:** [thought-experiment-2026-10-10.md](thought-experiment-2026-10-10.md)

## Decisions

| Question | Answer |
|---|---|
| Stack | Rust core with Swift bindings, the Peel shape: a core crate, a CLI, an FFI crate, a Swift package wrapping it. SQLite through rusqlite. |
| Repo | New standalone repo `semantic-core`. The existing `~/Projects/semantic-core/docs` becomes its docs folder. |
| Reflective Rust | Vendor the envelope rules in-tree. No path dependency on the unpublished crates. Pin the RR golden vector as a conformance fixture. |
| First surface | CLI plus fixtures first. Finder Sync action and the companion inspector are Stage 2. |
| Execution | Cloud session for the Rust core, fixtures and CI. Mac session only for the Swift package, Finder Sync, the inspector and anything under the build lock. |
| Repo policy | Public. Commits as Zach. No Co-Authored-By trailers, no AI mentions anywhere in commits, PRs, docs or code, the same rule as reflective-rust and reflective-index. |
| Governance | Formal Project Bootstrap v0.6.0 templates at repo creation: project intent, project profile, authority. Register `sc-event` in the protocol registry only once a conformance command actually runs. |
| Stage 1 scope | The nine proofs and fixture list in v0.2.2 section 11, plus three additions: the Swift golden-vector proof stays in Stage 1; Peel archive-member fixtures from a static ZIP, no PeelFS mount required; a Spotlight `IndexedEntity` spike with the non-disclosure test. |

Zach selected both "Stage 1 exactly as written" and the three additions. Read as: the nine proofs are not trimmed, and the three additions are appended.

## Repo layout at creation

```text
semantic-core/
  Cargo.toml                 workspace
  crates/
    sc-core/                 identity registry, relationships, assertions, observations, event log, SQLite views, resolver
    sc-envelope/             sc-event-0.1 envelope: JCS, sha256, Ed25519, access projection, freshness; RR golden vector test
    sc-vocab/                sc-vocab-1 registry, class constraints, inverses, unknown-predicate preservation
    sc-cli/                  link, move-resolve, inspect, retract, export, import, why
    sc-ffi/                  C ABI for Swift
  swift/
    SemanticCore/            Swift package over sc-ffi; golden-vector and signature-verify test
  fixtures/
    moves/ copies/ edits/ deletes/ ambiguous/ containers/ access/ vectors/
  docs/                      the three documents above plus FPB files
  PROJECT_INTENT.md  PROJECT_PROFILE.json  AUTHORITY.md   (FPB v0.6.0 templates)
  .github/workflows/ci.yml   cargo test, fixture suite, Swift test on a macOS runner
```

## Stage 0 checklist (done or remaining)

- Distance from Gray v1.0 located and cited. Done.
- RSP protocol version, canonicalization, signature construction and golden vector identified. Done.
- macOS 27.0 SDK availability for App Intents indexing, Foundation Models, Vision, FSKit, DiskImageKit verified. Done. No Evaluations framework.
- Remaining: DiskImageKit attach semantics; document-identifier behavior on non-APFS, network and iCloud volumes; whether the Swift package calls the FFI or re-implements the envelope for the golden-vector test (decision: calls the FFI for everything, and additionally re-computes the canonical hash in Swift for the vector so the boundary is proven both ways).

## First tasks in order

1. Create the repo with the FPB files, workspace, CI and the three docs.
2. `sc-envelope`: canonical JSON, hash, sign, verify, access projection, freshness. Pass the RR golden vector. Add `sc-event-0.1` vectors.
3. `sc-core` schema: things, artifacts, revisions, instances, presentations, relationships, assertions with polarity, observations, events, policy. Hash-chained event log. Materialization with the four relationship states.
4. Resolver with the three outcomes and the D2 rules. Fixtures for rename, move, copy, edit, delete, restore, ambiguous, missing volume.
5. `sc-cli` for the Stage 1 demonstration. The `why` command prints the assertion, evidence, resolution evidence and history for any relationship.
6. Container fixtures from a static ZIP: member identity, presentation lifecycle, extracted instance with derivation.
7. Access fixtures: restricted direct edge, transitive edge, count, search hit, export.
8. `sc-ffi` and the Swift package with the golden-vector and verify tests.
9. Spotlight `IndexedEntity` spike in a throwaway macOS target with the non-disclosure test.
10. Stage 1 gate: all nine proofs plus the three additions green in CI. Then Stage 2 on the Mac.

## Peel and PeelFS intermix (assessed 2026-10-10, not decided)

Two directions are worth it, one is not.

- **peel-core as an observation source.** `peel probe` already enumerates archive members with sizes and checksums as JSON. Once `sc-envelope` exists, a `peel probe --sc` flag emits those as `observed` events with Peel as the authority. That replaces the static-ZIP container fixtures with a real producer at almost no cost and makes Peel the first non-fleet consumer of the envelope discipline, which its own Reflective Rust note already plans.
- **PeelFS as the live presentation source.** PeelFS holds the mapping from item id to member path and owns the mount lifecycle. A hook that emits presentation events on mount, enumeration and unmount, behind a preference, gives decision D7 a real source: session-scoped ids, validity windows, unavailable on unmount, re-binding through the member. This is the only way to test D7 against a real mount rather than a fixture.
- **Not worth it: PeelFS as the semantic volume.** Exposing collections of things as an FSKit volume is the File Provider idea with a restricted entitlement, SIP and signing friction, and read-only semantics. It stays off the table until Stage 4 and would be File Provider then, not FSKit.

Repos stay separate. Peel depends on nothing from Semantic Core until `sc-envelope` has vectors; then Peel adds the two emitters as optional features. The Rust-core-plus-Swift shape, `scripts/build-app.sh` and the FFI pattern are copied from Peel at repo creation.

## Rules that carry over from other projects

- Fixtures use fictional names only, never household names.
- No secrets in argv on any host; tokens on stdin.
- Test against the merged payload shape, never a chat-agreed shape.
- Python 3.10 on titan if any helper runs there; this project should not need titan at all.
- Mac work takes the build lock and waits for the queue; never a bare `.build` binary.
