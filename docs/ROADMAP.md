# Roadmap

Stages follow section 11 of the [definition](semantic-core-definition-v0.2.2.md). Each stage ends with executable proofs, never a claim.

## Stage 0 — verification (done)

The verdict model, the envelope reference and its golden vector, and the macOS 27 SDK surfaces are identified and recorded in the definition's decisions.

## Stage 1 — portable core

1. Repository, workspace, CI, fixtures layout.
2. `sc-envelope`: canonical JSON, hashing, Ed25519, access projection, freshness; the reference golden vector reproduces.
3. `sc-vocab`: the registered predicate vocabulary with class constraints, inverses and unknown-predicate preservation.
4. `sc-core`: schema, hash-chained event log, materialization with four relationship states, the resolver with three outcomes and recorded evidence.
5. `sc-cli`: link, move-resolve, inspect, retract, export, import, why.
6. Container fixtures from a ZIP built at test time: member identity, presentation lifecycle, extracted instances.
7. Access fixtures: restricted direct, transitive, count, search, export; withheld never reveals existence.
8. `sc-ffi` and the Swift package: the golden vector and signature verification reproduced across the boundary. Implemented and tested: `crates/sc-ffi` (envelope and store C ABI) and `swift/SemanticCore` (golden vector both ways, CryptoKit verification, Stage 1 demonstration through the Swift store).
9. A Spotlight indexed-entity spike with the non-disclosure test. Spike done: `swift/SemanticCore` target `SemanticCoreSpotlight` builds a `ThingEntity: AppEntity, IndexedEntity` from a viewer-projected inspect and proves nothing withheld reaches the attribute set; live Spotlight registration needs an app bundle and is Stage 2.

Stage 1 is complete when the nine proofs in the definition and the fixture list are green in CI.

## Stage 2 — native macOS surface

Finder Sync action, a separate inspector, a minimal surface descriptor with participation reasons and access-first resolution. Extensions degrade gracefully when absent.

## Stage 3 — qualification and richer expression

A narrowly scoped qualification profile with mutation tests, surface and expression adapters, declared consequential actions, optional model-assisted proposals.

## Stage 4 — ecosystem adapters

Fundamental Engine spatial view, assistant and editor projections, archive observations, host interoperability, authorized cross-device exchange.
