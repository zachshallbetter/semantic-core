# Semantic Core — Product and Architectural Definition

**Version:** 0.2.1 (proposed)  
**Date:** 2026-10-10  
**Status:** Design definition; not an implementation or conformance claim. **Not started.** Stage 0 has not begun; this revision folds in the corrections from [thought-experiment-2026-10-10.md](thought-experiment-2026-10-10.md) and the review that followed it.
**Supersedes:** [v0.2.0](semantic-core-definition-v0.2.0.md), kept for history  
**Initial host:** macOS  
**Portable consumers:** Finder integration, Jellybean, ASE, Peel, Titan, Fundamental Engine, and future applications

## 1. Definition

**Semantic Core is a portable, local-first identity and relationship substrate for files, information artifacts, and real-world or conceptual things.** It maintains stable identities, typed relationships, observations, assertions, provenance, authority boundaries, and a reversible history of changes independently of filenames, filesystem locations, rendering systems, and AI models.

**Files have locations. Information has identity. Things have relationships.**

A receipt, warranty, manual, and service record can refer to the same dishwasher even when stored in unrelated directories. Moving a file does not by itself sever its justified relationship. Copying a file does not create the same filesystem instance. Identical hashes do not prove identical semantic identity.

Semantic Core is **not** a replacement filesystem, Finder modification framework, general-purpose knowledge oracle, AI agent, Fundamental Engine field runtime, or protocol-governance engine. It does not determine truth solely from model output, content similarity, or signatures.

## 2. Goals and non-goals

### Goals

- Operate offline with no mandatory account, server, cloud synchronization, model, or always-on daemon.
- Separate things, logical artifacts, revisions, and physical file instances.
- Maintain typed relationships whose meaning survives file relocation where evidence permits.
- Preserve the distinctions among observation, interpretation, candidate assertion, qualification, authorization, and committed assertion.
- Retain append-only logical event history, including rejected, retracted, and superseded claims.
- Enforce access controls before graph traversal, surface participation, indexing, and export.
- Provide a portable, versioned interchange format with integrity and optional signatures.
- Support native operating-system adapters and optional advanced visualization without coupling core correctness to them.
- Prove interoperability and invariants through executable conformance fixtures.

### Non-goals for v0.2

- Replacing Finder's native window layout or inserting arbitrary columns, panels, or relationship lines into Finder.
- Building a complete EQP engine, ICP runtime, FCI implementation, SSS resolver, or SES expression engine as prerequisites.
- Automatically merging entities because of a hash match or model prediction.
- Treating all inferred links as accepted truth.
- Implementing arbitrary filesystem surveillance on iOS or assuming universal Finder-selection observation.
- Full multi-device synchronization, household federation, or unattended consequential actions in Stage 1.

## 3. Core ontology

| Entity | Definition | Identity rule |
|---|---|---|
| **Thing** | Real-world or conceptual referent | Opaque semantic ID |
| **Artifact** | Logical unit of information/content | Opaque semantic ID |
| **Revision** | Particular version of an artifact | Distinct revision ID; may reference content digest |
| **Instance** | Independently persisted occurrence of a revision or artifact at a filesystem location | Distinct instance ID; path is mutable observation |
| **Relationship** | The canonical typed directed association between a subject and an object | Relationship ID, distinct from any assertion about it |
| **Observation** | Source-attributed report of a condition at a time | Distinct observation ID |
| **Assertion** | An actor-attributed claim about identity, an attribute, or a relationship, with its own evidence and lifecycle | Distinct assertion ID; many assertions may concern one relationship |
| **Presentation** | A transient appearance of an artifact or revision at a location that exists only while a mount, session, or remote connection does | Presentation ID with a validity window; never promoted to an instance implicitly |
| **Event** | Immutable logical record of a transition or action affecting graph state | Distinct event ID |
| **Actor** | Person, application, agent, or process responsible for an action | Separate from signing key |
| **Authority** | Policy or grant governing permitted action | Explicit, independently checked |
| **Trust class** | How the system may treat a source or instance, independent of where it came from and whether it is intact | Attribute on instances, presentations, and observation sources; never inferred from provenance or a signature |

**Relationship is not assertion.** Two actors asserting `Warranty.pdf DOCUMENTS Dishwasher` describe one relationship through two assertions with separate evidence. Retracting one leaves the other standing. The materialized graph shows a relationship as present when at least one accepted assertion concerns it, and the inspector can list every assertion behind it. This split is required before any schema is written; it is what federation, conflicting evidence, and independent authority rest on.

**Reserved, not implemented.** The names `Episode` (what was in use together), time facets beyond observation time (`due`, `about`, `expires`), and known-absence records are reserved in the vocabulary so later stages do not bolt them on. They are out of scope for Stage 1.

Use opaque identifiers such as ULIDs. File paths, hashes, device IDs, and signing keys are evidence or attributes, **not canonical semantic identities**. Signing-key rotation must not change object IDs.

Relationships use a **versioned registered vocabulary** with source/target constraints, direction, inverses when defined, and unknown-type preservation in interchange. Initial candidates: `INSTANCE_OF`, `REVISION_OF`, `DOCUMENTS`, `REFERENCES`, `DERIVED_FROM`, `SUPERSEDES`, `RELATED_TO`. These names are *proposed local vocabulary*, not inherited normative protocol enums.

## 4. Distinct states and decision boundaries

Do not collapse the following pipeline into one confidence score or status:

```text
Filesystem / content observation
    -> identity resolution (RESOLVED | AMBIGUOUS | UNRESOLVED)
    -> candidate assertion
    -> evidence evaluation and bounded qualification
    -> separate authorization / acceptance decision
    -> committed assertion in materialized graph
    -> authorized contextual surface
    -> host-specific expression and action
```

**Three decision vocabularies, kept apart.**

| Mechanism | States | Question it answers | Attaches to |
|---|---|---|---|
| Identity resolution | `RESOLVED`, `AMBIGUOUS`, `UNRESOLVED` | Which object is this? | An observation against the registry |
| EQP qualification | `PASS`, `BLOCKED`, `INCONCLUSIVE`, `NOT_RUN` | What claim does this evidence justify? | A qualification record for a candidate and claim |
| Commitment gate (Distance from Gray v1.0) | `PASS`, `FAIL`, `ESCALATE` over a per-property vector of `SATISFIED`, `VIOLATED`, `GRAY` | May this commitment proceed under its pinned requirements, on the evidence admissible by its clock? | A declared commitment such as accepting an assertion, exporting a graph, or sharing a thing. Never an edge. |

The formal model of record is `judgment-machines/books/01-the-last-undo/measurement/formal-model.md`, version 1.0, ratified 2026-08-30. Its rules that bind here: the required set is derived from the commitment and its use, never from the artifact; GRAY routes to ESCALATE and never to silence; authority may establish a judgment-bearing property through a recorded resolution but cannot manufacture a missing fact; the receipt carries the verdict and state vector. Placing `SATISFIED` on a relationship as an intrinsic property contradicts this model.

**Identity resolution is not EQP qualification.** `RESOLVED` does not imply `PASS`; `AMBIGUOUS` does not necessarily imply `INCONCLUSIVE`; `UNRESOLVED` does not automatically imply `BLOCKED`. EQP's `PASS`, `BLOCKED`, `INCONCLUSIVE`, and `NOT_RUN` retain their separate meanings when an EQP-compatible qualification profile is actually used.

**Qualification is not promotion.** Evidence may justify a bounded claim without authorizing its acceptance. The weakest justified claim governs. A signature attests integrity and origin under a trust model, not factual truth.

**Event is not assertion.** An assertion such as “Warranty.pdf documents Dishwasher” may have events `ASSERTION_PROPOSED`, `EVIDENCE_ATTACHED`, `QUALIFICATION_RECORDED`, `ASSERTION_ACCEPTED`, and `ASSERTION_RETRACTED`. Retraction changes the current projection without erasing the record.

**Absence is not negation.** Missing, unavailable, withheld, ineligible, and absent information must remain distinguishable, particularly in restricted views.

## 5. Storage and portability

Stage 1 uses a library with local SQLite storage, an append-only *logical* event log, and materialized graph/query tables. The library exposes transactions, bounded queries, typed graph operations, and read-only projections. It may use filesystem notifications opportunistically but must reconcile after missed notifications or offline periods.

A portable event envelope should carry at least:

- Schema/protocol version and event ID.
- Subject/object identifiers and typed payload.
- Actor and separately identified signing key.
- Source and provenance classification.
- Event time, observation time, and freshness/validity information where applicable.
- Access classification and authorization context/reference.
- Referenced evidence and content digests where applicable.
- Canonical serialization and optional Ed25519 signature.

Reflective Rust's Runtime Semantic Projection (RSP) is a **reference for envelope discipline**, not the semantic graph ontology. Preserve provenance classes `compiler-certified`, `declared`, `derived`, and `observed` where adopted; never silently promote between them. CSG is compiler-specific and OSG is operational/fleet-shaped; neither should be renamed as the file graph.

The reference is protocol version `rr-rsp-0.1` in the `reflective-rust-protocol` crate: JCS canonical JSON, SHA-256 identity, Ed25519 with a domain-separated signature, a golden vector in the crate, access classes `public < member < owner < administrator` as a disclosure projection rather than authentication, and observation envelopes with `observed_at`, `expires_at`, and a status of `synchronized | drifted | missing | unavailable | unknown`. The crate is unpublished and untagged, and its own documentation states that where prose and code disagree the weaker claim governs.

**Compatibility is unproven until tested.** A Swift envelope implementation must match the Rust reference's exact canonical JSON, hashing, signing, and verification vectors. A similar JSON shape is not sufficient.

## 6. Protocol responsibility map

These are **design references and adapter boundaries**, not mandatory runtime dependencies or assertions of protocol conformance.

| Source | Governs or informs | Must not be mistaken for |
|---|---|---|
| **FCI v0.5.0** | Distinctions among ontology, representation, observation, evidence, interpretation, projection, and measurement | A file graph database or ready-made projection runtime |
| **RSP / Reflective Rust** | Portable envelope, provenance, integrity, access and observation discipline | File-oriented ontology or compiler certification without extractor |
| **EQP v0.1.0** | Candidate/claim/evidence/evaluation/qualification separation; weakest justified claim | Acceptance or promotion authority |
| **ICP v0.1.0** | Declared consequential interactions, availability vs permission vs enablement vs exposure, checks, receipts, recovery | Every read-only query or a proven production execution engine |
| **SSS v0.1.0** | Contextual, read-only, access-safe resolution of a warranted semantic surface | Canonical truth, authorization grant, or graphical renderer |
| **SES v1.0.0** | Preserving meaning while expressions vary | Canonical data ownership or surface selection |
| **SPS v0.1.0** | Separation of published truth and presentation, where publication applies | Mandatory local storage format |
| **ESP v0.3.0** | Externally dependent commitments and selected recovery concepts | General governing protocol for ordinary filesystem mutations |
| **IEPE v0.2.0 / RDP v0.1.0** | Engineering evidence, release/promotion and research claim discipline | Product runtime dependencies |
| **Fundamental Engine** | Optional fields, forces, metrics, spatial interaction and visualization | Evidence adjudicator or owner of canonical assertions |

The October 9 protocol bundle is a generated projection; its canonical source files retain authority. The protocol register is dated August 30 and does not prove current implementation maturity. The bundle also contains duplicated recipient/engagement material under `rcp.txt` and `edp.txt`; do not treat those as separate established dependencies.

### SSS/SES boundary (critical revision)

**Canonical graph -> access projection -> SSS semantic surface resolution -> SES/host expression -> user-visible interface.**

SSS asks: *Given available semantic state and a declared situation, what semantic surface is warranted here?* Its resolver is read-only with respect to canonical state. Access projection occurs **before** participation resolution. Context changes selection/emphasis, not meaning. Every included member needs an inspectable participation basis. Relationship presence does not imply causal coupling. Exposing an operation does not imply it is permitted, enabled, or executed.

SES protects the meaning across Finder badges, Jellybean cards, ASE task views, Quick Look previews, and Fundamental Engine visualizations. Neither SSS nor SES owns graph truth.

A **projection contract** should record source scope, principal/capabilities, preserved/transformed/omitted/introduced information, provenance, and failure/withholding semantics. A filtered result must never imply a restricted relationship does not exist.

## 7. macOS integration: three surfaces

### Level 1 — Native Finder integration

Use documented mechanisms where appropriate: Finder Sync contextual commands and monitored-directory badges; Quick Look previews for registered content types; thumbnail extensions; Spotlight/Core Spotlight indexing; App Intents actions/entities; supported file metadata and tags. Finder Sync is sync-oriented and does **not** provide arbitrary file-cell rendering or universal selection tracking. Each extension is optional; failure must not compromise core state.

### Level 2 — Semantic inspector

A separate SwiftUI companion window, launched explicitly from Finder, shows the current object's authorized relationships, evidence provenance, resolution status, and available actions. Following arbitrary Finder selection may require constrained Accessibility integration and user consent; it is **not** assumed to be a Finder Sync capability.

### Level 3 — Semantic workspace

A dedicated interface offers graph navigation, conflict inspection, identity reconciliation, and historical events. Fundamental Engine may optionally consume an authorized graph/surface adapter to produce field bodies, relationship forces, metrics, and spatial layouts. It cannot independently mutate canonical truth.

File Provider may later expose a **managed** semantic collection in Finder; it is not a general API for arbitrary virtual directories within all existing folders.

## 8. macOS 27 / Golden Gate SDK research candidates

The earlier discussion identified App Intents entity schemas and view annotations, Spotlight, Core AI, Foundation Models, Vision, Evaluations, DiskImageKit, and Quick Look/RealityKit as opportunities. **Treat their exact macOS 27 names, capabilities, availability, deployment targets, entitlements, and supported data flow as unverified until checked against the installed SDK and current Apple documentation.** They are research tracks, not Stage 1 dependencies.

| Track | Proposed experiment | Acceptance condition |
|---|---|---|
| Spotlight / App Intents | Index and retrieve one authorized `Thing` with related files | Verify discoverability and non-disclosure for unauthorized principals |
| Foundation Models / Vision | Extract serial number and propose candidate relationship | Evidence retained; no automatic acceptance |
| Local model runtime | Compare deterministic matching to model-assisted candidates | Measured precision/recall and false-merge rate |
| Evaluation tools | Run ambiguous, contradictory, unreadable, restricted fixtures | Failures and abstentions remain explicit |
| File Provider | Expose a bounded managed semantic collection | No assumption of global Finder view control |
| DiskImageKit / Peel | Determine actual supported image operations | No unsupported archive-format claims |

## 9. Security and authority

- Apply authorization **before** graph traversal, ranking, search indexing, SSS participation, summaries, counts, exports, and agent access.
- Do not leak restricted edge existence through identifiers, hashes, badges, explanations, or aggregate counts.
- Treat file contents and model outputs as untrusted data, never instructions to an agent.
- Keep four questions separate on every source and instance: **provenance** (where it came from), **integrity** (whether it changed since verification), **trust class** (how the system may treat it), and **authority** (what the source or actor may do). A verified signature does not raise trust; a trusted application can still emit a wrong observation; a locally created file carries no authority over an agent.
- **Stage 1 policy default:** one local owner, every entity private by default, no implicit sharing. This is a policy, not an assumption that actor, owner, device, and signing key are one identity. Changing it later is a policy change, not a schema change.
- Separate actor identity, cryptographic key identity, policy authority, and operation permission.
- Distinguish action **available**, **permitted**, **enabled**, **exposed**, and **believed available** per ICP.
- Require explicit authority and an auditable receipt for consequential changes; record partial failure and recovery behavior.
- Use compensating events for undo; do not erase history to simulate reversal.
- Do not assume a shared envelope implies a shared access grant across devices or applications.

## 10. Consumer responsibilities

| Consumer | Role | Boundary |
|---|---|---|
| Finder | Native file navigation and explicit semantic actions | No canonical graph ownership or custom embedded inspector |
| Jellybean | Household-oriented, access-scoped entity and document cards/actions | No authority inferred from discovery |
| ASE | Task-scoped evidence and file relationships | No unrestricted filesystem access |
| Peel | Archive/container-member observations and associations; PeelFS mounts | A mounted member is a presentation, not an instance; extraction creates a new instance with `DERIVED_FROM` provenance |
| Titan | Operational observations and explicitly federated references | No implicit personal-file access |
| Fundamental Engine | Optional spatial computation and visualization | No truth adjudication or assertion acceptance |

### Containers and mounts (PeelFS case)

| Object | Treatment |
|---|---|
| Archive file | Artifact with one or more filesystem instances |
| Archive member | Artifact or revision identified within its containing archive |
| Mounted member | Presentation with a validity window bound to the mount; it may carry a transient locator but is not persisted |
| Extracted member | New instance, `DERIVED_FROM` the member, provenance `derived` |
| Unmounted archive | The member's presentation becomes `unavailable`; nothing is deleted |

The same treatment applies to disk images, package contents, remote volumes, and offline cloud placeholders.

## 11. Stage plan and executable proofs

### Stage 0 — Verification

Pin the canonical protocol revisions actually used. The Distance from Gray v1.0 model and the RSP protocol version and golden vector are already identified (sections 4 and 5); what remains is the Swift reproduction of the golden vector and verification of macOS APIs against an installed SDK. Record unresolved discrepancies rather than silently harmonizing them.

### Stage 1 — Portable core (no AI required)

Implement identity registry, typed relationships, observations, assertions, event log, SQLite views, scoped authorization, and a minimal API. Prove:

1. A link between two files survives rename/move **when identity evidence is sufficient**.
2. Copying a file creates a distinct instance; identical hashes do not silently merge semantic IDs.
3. Edit, delete, restore, and missing files produce explicit resolution outcomes.
4. Conflicting or insufficient evidence remains visible as uncertainty, not false certainty.
5. Assertion acceptance requires separate authority; qualification cannot self-promote.
6. Restricted relationships do not leak through queries, counts, exports, or derived surfaces.
7. Retraction appends a compensating event and preserves original evidence.
8. Rust and Swift agree on canonical bytes and Ed25519 verification for fixed vectors.
9. All core operations work offline.

**Demonstration:** Link two files -> move one -> resolve and inspect relationship -> retract -> inspect history. This must work without FE, models, File Provider, or a full protocol engine.

### Stage 2 — Native macOS surface

Add explicit Finder action, separate inspector, and a **minimal SSS-inspired surface descriptor** with participation reasons and access-first resolution. Verify that disabled/missing extensions degrade gracefully. Add Quick Look or Spotlight only after independent tests.

### Stage 3 — Qualification and richer expression

Implement a narrowly scoped EQP-compatible qualification profile with mutation tests, SSS/SES adapters, ICP-style consequential action declarations, and optional model-assisted proposals. State the actual conformance level, not an inferred one.

### Stage 4 — Optional ecosystem adapters

Introduce Fundamental Engine spatial view, Jellybean cards, ASE task projections, Peel archive observations, Titan interoperability, and eventually authorized cross-device exchange. Keep ontology and authority boundaries explicit.

## 12. Required conformance fixtures

- Rename within volume; move across volumes; copy; modify content; delete; restore; stale path.
- Same content hash but different semantic subjects; same subject with multiple revisions/instances.
- Ambiguous identity; unavailable source; conflicting observations; outdated evidence.
- Observed vs declared vs derived provenance; tampered signature; rotated signing key.
- Candidate qualified but unauthorized; permitted action disabled; exposed action not executable.
- Restricted direct edge; restricted transitive edge; restricted count; restricted search hit.
- Surface member included with traceable basis; member withheld without existence leak.
- Retraction, replay, crash recovery, idempotent import, and offline operation.
- Two independent assertions of one relationship; retract one; the relationship remains present with the other's evidence.
- Mounted archive member browsed, archive unmounted, archive remounted: presentation goes unavailable and returns; no instance is created or deleted; an extracted copy is a distinct instance with derivation provenance.
- Verified signature on an instance with an external trust class: trust unchanged, contents still untrusted to an agent.

## 13. Open decisions

1. Exact canonical envelope version, schema and serialization rules shared with Reflective Rust.
2. Stable cross-volume instance identity policy and evidence thresholds.
3. Relationship vocabulary registration/versioning and extensibility rules.
4. Whether initial event signatures are mandatory locally or mandatory only on interchange.
5. Minimal SSS surface descriptor versus adoption of the full normative candidate.
6. Policy authority and access model for single-user Stage 1 versus later household federation.
7. Transient presentation identity: whether presentations receive durable IDs that can be re-bound on remount, or only session-scoped locators. Whatever the answer, the durable identity of the contained artifact and the identity of its temporary presentation stay separate; they solve different problems.
8. Verified macOS 27 SDK availability and App Intents indexing behavior.
9. Relationship materialization under contradiction: the "present while any accepted assertion supports it" rule does not yet distinguish a supporting assertion from an accepted assertion that disputes the same relationship. A materialization policy for conflicting accepted assertions is needed before the rule is encoded, or conflicting claims collapse into a misleading binary.

## 14. Architectural statement

**Semantic Core owns stable identities, typed relationships, evidence-linked assertions, and their event history. Reflective Rust informs portable envelope discipline. EQP informs what evidence justifies, without granting acceptance. FCI informs epistemic distinctions. SSS selects an authorized, inspectable semantic surface; SES preserves meaning across expressions. ICP governs consequential actions. Finder and other applications present and act on authorized results. Fundamental Engine is an optional computational and spatial consumer.**

The first product proof is deliberately modest: a relationship survives a justified file move, remains inspectable through a native macOS entry point, does not leak through unauthorized views, and can be retracted without rewriting history.

---

**Changes from v0.2.0:** relationship and assertion identities separated; presentation added for mounted and virtual locations; trust class added and separated from provenance, integrity, and authority; three decision vocabularies tabulated with the Distance from Gray v1.0 model cited by path; RSP protocol facts recorded; single-owner default stated; container and mount treatment added; reserved names for episode, time facets, and known absence; fixtures extended; open decision 7 closed and replaced.

**Source basis:** October 9, 2026 generated protocol bundle (`sss.txt`, `ses.txt`, `eqp.txt`, `icp.txt`, `fci.txt`, `idx.txt` and related corpora), plus the supplied Semantic Core/Finder/Golden Gate architectural discussion. Proposed implementation choices are labeled as such and are **not** claims that the referenced protocols or macOS SDK integrations have been implemented or verified.
