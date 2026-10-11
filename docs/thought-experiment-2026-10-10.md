# Semantic Core — thought experiment record

**Date:** 2026-10-10
**Status:** Not started. This is a record of a design conversation, kept so the reasoning is not lost. Nothing here is a commitment, a roadmap item, or a conformance claim.
**Companion:** [semantic-core-definition-v0.2.0.md](semantic-core-definition-v0.2.0.md) is the proposed definition. This file records how it was reached, what was verified against real sources, and what the definition still gets wrong or leaves open.

## 1. How the idea moved

The conversation started from "what if Finder had better relationships, tags and identifiers for Jellybean and titan" and ended at a portable library. The turns, in order:

1. **Finder metadata.** Stable ids in xattrs, typed provenance, owner and audience on every file, typed edges, tags as relations, event time beside file time, card verbs on files. Useful, but xattrs do not survive transfer and Finder exposes almost none of it.
2. **A household substrate.** Everything a member touches is a thing with one schema; files are one kind of body; the box is the canonical store; ingest is the only door. Too big, and it made the assistant the owner of the data.
3. **A personal graph, federated.** One person, their devices, a signing key as root. The household becomes a union of signed share edges. Sharing is explicit, nothing is shared by default. This fixed the privacy problem and was kept.
4. **The merged shape.** A library and a file format, not a service. Thing, artifact, revision, instance. Subject, predicate, object with asserter, confidence, status and evidence. Owner, audience, trust, time facets and tombstones in the first schema. Mac indexes, iOS reads.
5. **Tightenings.** Object identity separate from actor and signing identity. Observations separate from assertions. Resolution outcomes resolved, ambiguous, unresolved. Compensating undo on an operation log. Access control as an enforced boundary, not a column. "Why does this relationship exist" as a core capability.
6. **Protocol grounding.** The verdict and qualification machinery that one draft attributed to Fundamental Engine actually lives in the protocols family. Reflective Rust's envelope turned out to be the closest existing artifact to what the core needs.

## 2. What was verified, and where

Each of these was checked against the source named, not taken from a summary.

**Fundamental Engine is a renderer, not an evaluator.** [llms-full-fe.txt](../../fundamental-engine/.agents/llms-full-fe.txt) contains no SATISFIED, VIOLATED or GRAY. The Evidence Fields paper states that Fundamental visualizes evidence and does not adjudicate it. Confidence and contradiction are host-supplied and become forces through `bindData()`. The nearest verdict-like things are internal and unshipped: a five-status property validator in the world-kernel experiment, a three-valued projection invariant, a planned admissibility gate for comparing worlds. The Govern verb's read-only agent view and the Relationship Registry are the relevant shipped pieces. Correct role: an optional surface.

**The verdict vocabulary is Distance from Gray.** The formal model the assessment cites was located at [judgment-machines/books/01-the-last-undo/measurement/formal-model.md](../../judgment-machines/books/01-the-last-undo/measurement/formal-model.md), version 1.0, ratified 2026-08-30. This resolves open decision 7 in the definition. Its rules that matter here:

- An assurance state is per required property, per commitment, clocked: `state(r, K, t_K)` in SATISFIED, VIOLATED, GRAY. Admissible evidence is bounded by the commitment clock.
- The required set is derived from the commitment and its use, never from the artifact.
- The gate is contract-defined and returns PASS, FAIL or ESCALATE. GRAY routes to ESCALATE, never to silence. A VIOLATED property does not universally mean FAIL.
- Authority can establish a judgment-bearing property through a recorded resolution. It cannot manufacture a missing fact.
- The receipt makes the verdict and state vector portable.

Implication for the core: a verdict belongs to a commitment, such as accepting an edge, exporting a graph or sharing a thing. It does not belong to an edge. Edges carry evidence and a status. Commitments pin a required set and get a receipt. Putting SATISFIED on a relationship would be the artifact-intrinsic gray the model forbids.

**EQP owns qualification; its dispositions are a different vocabulary.** [EQP v0.1.0](../../_protocols/evidence-qualification-protocol/01-EQP-SPECIFICATION.md) qualifies what evidence justifies about a candidate under a profile, returns PASS, BLOCKED, INCONCLUSIVE or NOT_RUN, applies the weakest-justified-claim rule and keeps promotion authority outside itself. The definition is right not to map resolver outcomes onto these. The [protocol assessment](../../_protocols/ASSESSMENT.md) of 2026-09-29 records that EQP has a formal model and no engine, and that IEPE carries a second qualification semantics with no mapping between them. If the core ever adopts one, it should be EQP, with the IEPE mapping written down.

**ICP, FCI, SSS and SES hold the remaining boundaries.** ICP declares consequential interactions with receipts and recovery and has no runtime yet. FCI already defines the observation, semantic assignment, interpretation and projection split, and its projection contract is the same preserve, transform, omit, introduce structure SES uses. SSS's invariants confirm the definition's claims: resolution is read-only against canonical state, access projection precedes participation, every member has a traceable basis. SSS has empty schema and conformance folders and no implemented consumer, so the definition's "minimal SSS-inspired surface descriptor" would be SSS's first resolver, not an adoption of a proven one.

**Reflective Rust's RSP envelope is shipped code.** The protocol crate, version 0.1, unpublished and untagged, defines schema version, subject id, compatibility, provenance, minimum access and body. Provenance classes are compiler-certified, declared, derived and observed, and consumers must not promote one into another. Identity is canonical JSON hashed with SHA-256. Signing is Ed25519 with the key supplied from outside the envelope. Observations are separate envelopes with a freshness window and a status of synchronized, drifted, missing, unavailable or unknown. Access classes are a disclosure projection, not authentication. A golden vector exists. The OSG node kinds are runtime, capability, authority, contract, data store, provider and model, with free-string edge relationships, so it is fleet-shaped and not a file ontology. The doc ranks itself at the lowest authority level and states that the weaker claim governs. Titan's fleet contract has been mapped into it and tested locally. No compiler extractor exists.

**Formal Project Bootstrap** is at version 0.6.0 in [_formal-project-bootstrap](../../_formal-project-bootstrap/formal-project-bootstrap/). ESP depends on it and the registry does not list it. Its intent, profile and authority templates are what this project would bootstrap with if it ever started. Noted only.

## 3. Review of the v0.2.0 definition

**What holds.** The ontology, the non-collapsed pipeline in section 4, the protocol responsibility map, the three macOS surfaces, the security list, the stage plan and the fixture list are all consistent with the verified sources. The architectural statement in section 14 is the right one: the core owns identities, relationships, evidence-linked assertions and history, and nothing else decides how they appear.

**Corrections and additions.**

- **Open decision 7 is closed.** The formal model is located and summarized above. The definition should cite it by path and version and state that verdicts attach to commitments, not edges.
- **Three vocabularies, kept apart.** Resolver outcomes (resolved, ambiguous, unresolved), EQP dispositions (PASS, BLOCKED, INCONCLUSIVE, NOT_RUN) and Distance from Gray gate verdicts (PASS, FAIL, ESCALATE) are three different things. The definition keeps the first two apart. It should name the third and say when it applies: only at a declared commitment with a pinned required set.
- **Stage 0 RSP check is partly done.** Schema version `rr-rsp-0.1`, a golden vector in the crate, Ed25519 over JCS canonical JSON. What remains is the Swift port against that vector. The definition's warning that a similar JSON shape is not sufficient is correct.
- **Trust on instances.** The definition treats file contents and model output as untrusted data. It should also carry a trust marker on the instance itself, so a consumer inherits the untrusted-text contract from the graph instead of re-deriving it from provenance each time.
- **Single-user default stated.** Owner and audience default to the one person. The definition says access is enforced, which is right. It should say the default explicitly so federation later is a policy change and not a schema change.
- **Deferred ideas worth keeping a slot for.** Time facets beyond observation time (due, about, expires), tombstones with known-absence, and an episode or activity entity that groups what was in use together. None belong in Stage 1. The ontology should reserve the names so they are not bolted on later.
- **Fifth-system risk.** Adopting four protocols without engines is the risk the library-not-service decision was meant to avoid. The definition handles this correctly by borrowing definitions and declaring adapter boundaries. It should also say the inverse plainly: if built, the core would be the first engine that can fail for EQP's obligations and the first non-fleet consumer of RSP.

## 4. Peel and PeelFS

Peel is the nearest live project to this model and already uses some of its concepts.

- **PeelFS** is an FSKit read-only mount. Finder browses archive members as files without extraction. A mounted member has a path that exists only while the mount does. That is exactly the "unavailable volume is not a deleted file" fixture, and it is the first case where an instance lives inside a container artifact rather than on a volume.
- **Containers.** Peel's container doc already separates the archive from its members. In the core's terms the archive is an artifact with instances, each member is an artifact whose instance is addressed through the container, and extraction creates new instances with a `derivedFrom` edge to the member. The definition's consumer table says extraction does not preserve instance identity automatically. PeelFS sharpens this: browsing does not create an instance at all, extraction does.
- **Reflective Rust.** Peel's own note mirrors capability-shaped JSON in the CLI and MCP tools now and plans to emit RSP envelopes from `peel probe --rsp` later. A probe result is an observation with provenance. That makes Peel the natural first non-fleet RSP consumer, and its probe output the first automatic observation source that needs no model.
- **Finder surfaces.** Peel already ships Finder Services, Quick Look work and an FSKit extension. Whatever the core learned about Finder Sync, Quick Look and File Provider limits applies there first.

## 5. Decision

Not started. The corrections above, plus two from the review that followed this record, are folded into [semantic-core-definition-v0.2.1.md](semantic-core-definition-v0.2.1.md); v0.2.0 is kept for history. The two review additions: relationship identity is separated from assertion identity, so two actors asserting the same edge make two retractable claims about one relationship; and a mounted archive member is a transient presentation with a validity window rather than "not an instance at all", which was too strong in section 4 above. Stage 0 has not begun and no implementation exists.

Later the same day the nine open decisions were answered in [semantic-core-definition-v0.2.2.md](semantic-core-definition-v0.2.2.md), each with its evidence: the installed macOS 27.0 SDK headers and module interfaces, the Reflective Rust protocol and conformance crates, and the SSS corpus. Three items stay open there. Still not started.
