# Semantic Core — Product and Architectural Definition

**Version:** 0.2.2 (proposed)  
**Date:** 2026-10-10  
**Status:** Design definition; not an implementation or conformance claim. **Not started.** Stage 0 has not begun; this revision answers the open decisions of v0.2.1 with evidence from the installed macOS 27.0 SDK, the Reflective Rust crates, and the SSS corpus.
**Supersedes:** [v0.2.1](semantic-core-definition-v0.2.1.md) and [v0.2.0](semantic-core-definition-v0.2.0.md), kept for history  
**Initial host:** macOS  
**Portable consumers:** Finder integration, a household assistant and its apps, a code editor, an archive utility, Fundamental Engine, and future applications

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
| **Assertion** | An actor-attributed claim about identity, an attribute, or a relationship, with a polarity (`SUPPORTS` or `DISPUTES`), its own evidence, and its own lifecycle | Distinct assertion ID; many assertions may concern one relationship |
| **Presentation** | A transient appearance of an artifact or revision at a location that exists only while a mount, session, or remote connection does | Presentation ID with a validity window; never promoted to an instance implicitly |
| **Event** | Immutable logical record of a transition or action affecting graph state | Distinct event ID |
| **Actor** | Person, application, agent, or process responsible for an action | Separate from signing key |
| **Authority** | Policy or grant governing permitted action | Explicit, independently checked |
| **Trust class** | How the system may treat a source or instance, independent of where it came from and whether it is intact | Attribute on instances, presentations, and observation sources; never inferred from provenance or a signature |

**Relationship is not assertion.** Two actors asserting `Warranty.pdf DOCUMENTS Dishwasher` describe one relationship through two assertions with separate evidence. Retracting one leaves the other standing. The materialized graph shows a relationship as present when at least one accepted assertion concerns it, and the inspector can list every assertion behind it. This split is required before any schema is written; it is what federation, conflicting evidence, and independent authority rest on.

**Reserved, not implemented.** The names `Episode` (what was in use together), time facets beyond observation time (`due`, `about`, `expires`), and known-absence records are reserved in the vocabulary so later stages do not bolt them on. They are out of scope for Stage 1.

Use opaque identifiers such as ULIDs. File paths, hashes, device IDs, and signing keys are evidence or attributes, **not canonical semantic identities**. Signing-key rotation must not change object IDs.

Relationships use a **versioned registered vocabulary** with source/target constraints, direction, inverses when defined, and preservation of unregistered predicates as values (never as unknown fields; see decision 3). Initial candidates: `INSTANCE_OF`, `REVISION_OF`, `DOCUMENTS`, `REFERENCES`, `DERIVED_FROM`, `SUPERSEDES`, `RELATED_TO`. These names are *proposed local vocabulary*, not inherited normative protocol enums.

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

The formal model of record is the Distance from Gray formal model, version 1.0, ratified 2026-08-30 (a separate document, not yet published). Its rules that bind here: the required set is derived from the commitment and its use, never from the artifact; GRAY routes to ESCALATE and never to silence; authority may establish a judgment-bearing property through a recorded resolution but cannot manufacture a missing fact; the receipt carries the verdict and state vector. Placing `SATISFIED` on a relationship as an intrinsic property contradicts this model.

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

### SSS/SES boundary (critical revision)

**Canonical graph -> access projection -> SSS semantic surface resolution -> SES/host expression -> user-visible interface.**

SSS asks: *Given available semantic state and a declared situation, what semantic surface is warranted here?* Its resolver is read-only with respect to canonical state. Access projection occurs **before** participation resolution. Context changes selection/emphasis, not meaning. Every included member needs an inspectable participation basis. Relationship presence does not imply causal coupling. Exposing an operation does not imply it is permitted, enabled, or executed.

SES protects the meaning across Finder badges, assistant cards, editor task views, Quick Look previews, and Fundamental Engine visualizations. Neither SSS nor SES owns graph truth.

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
| DiskImageKit / archive mounts | Determine actual supported image operations | No unsupported archive-format claims |

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
| Household assistant apps | Access-scoped entity and document cards and actions | No authority inferred from discovery |
| Code editor | Task-scoped evidence and file relationships | No unrestricted filesystem access |
| Archive utility | Archive and container-member observations and associations; read-only mounts | A mounted member is a presentation, not an instance; extraction creates a new instance with `DERIVED_FROM` provenance |
| Household assistant host | Operational observations and explicitly federated references | No implicit personal-file access |
| Fundamental Engine | Optional spatial computation and visualization | No truth adjudication or assertion acceptance |

### Containers and mounts (read-only archive mount case)

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

Introduce the Fundamental Engine spatial view, assistant cards, editor task projections, archive observations, host interoperability, and eventually authorized cross-device exchange. Keep ontology and authority boundaries explicit.

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

## 13. Decisions

Answered 2026-10-10 against the installed macOS 27.0 SDK (`MacOSX.sdk`, build 26A428), the `reflective-rust-protocol` and `reflective-rust-conformance` crates, and the SSS corpus. Each decision names its evidence; where the evidence is a design judgment rather than a verified fact, it says so.

### D1. Envelope: same discipline, sibling protocol

**Decision.** Semantic Core events use their own protocol identifier, `sc-event-0.1`, with the RSP envelope discipline copied field for field: `schema_version`, `subject_id`, `compatibility {protocol, subject_schema}`, `provenance {kind, authority, source, source_hash, observed_at?, expires_at?}`, `minimum_access`, `body`. Canonicalization is JCS (RFC 8785); identity is `sha256:<hex>` of the canonical bytes; signatures are Ed25519 over `domain || key_id || 0x00 || canonical`, base64url without padding; every struct rejects unknown fields. The domain string is Semantic Core's own, not `reflective-rust:rsp:ed25519:v1`.

**Why not carry file-graph bodies inside `rr-rsp-0.1`.** `DescriptorBody` is a closed tagged enum of `type`, `runtime`, `observation`, `operational-graph`, and `validate()` rejects any other `schema_version`. Adding a body kind means forking or changing the crate, and the crate is unpublished. A sibling protocol keeps the envelope rules identical while leaving the crate alone.

**Conformance path.** The Swift implementation first reproduces the crate's golden vector exactly: subject `rust:type:Empty`, schema `type-1`, provenance `derived` from `reflective-rust-conformance`, source hash `sha256:503e43da…`, public access, empty struct body, expected hash `sha256:7c98ac356aa59dd29e466c6f6e5927eb5afd2ad85f283d59d920de3f7bf05a18`. That proves JCS, hashing and signing agree. An absent `layout` is omitted from the body, not serialized as null; only the omitting form reaches the recorded hash. An absent `layout` is omitted from the body, not serialized as null; only the omitting form reaches the recorded hash. Only then does `sc-event-0.1` get its own vectors.

**Observation rule carried over.** An `observed` event must carry `observed_at` and `expires_at`, with expiry later than observation; `declared`, `derived` and `compiler-certified` events must not. Expired observations may be kept and must not drive current-state decisions.

### D2. Instance identity across volumes: rules, not scores

**Evidence from the SDK.** `NSURLVolumeUUIDStringKey` is the volume's persistent UUID. `NSURLDocumentIdentifierKey` is kernel-assigned and identifies a document regardless of where it moves on a volume. `NSURLFileIdentifierKey` is the inode and the header says it is not stable across file systems or mounts. `NSURLFileResourceIdentifierKey` is not persistent across restarts. `NSURLFileContentIdentifierKey` identifies an APFS content stream and is useful for detecting clones within a volume only. `NSURLGenerationIdentifierKey` changes when content changes and persists across restarts.

**Decision.** The within-volume key is `(volume UUID, document identifier)`. No OS key survives a cross-volume move, so cross-volume resolution is evidence-based with these outcomes:

| Outcome | Rule |
|---|---|
| `RESOLVED` | `(volume UUID, document identifier)` matches a known instance; or exactly one candidate matches the revision's content hash **and** the previously known instance of that revision is gone. |
| `AMBIGUOUS` | The content hash matches more than one candidate; or it matches one candidate while the previously known instance still exists (that is a copy, not a move); or only name, size and modification time match. |
| `UNRESOLVED` | Nothing matches. The instance record is marked `missing`; the artifact and thing are untouched. |

Inode, resource identifier, name, size, modification time, an xattr stamp and an FSEvents rename are evidence that can move a case between rows; none of them alone produces `RESOLVED`. A matching hash never merges two artifacts; it can only bind an instance to a revision. These are rules so the inspector can state which one fired.

### D3. Vocabulary: registered, versioned, unknown kept as a value

**Decision.** Predicates live in a registry versioned as `sc-vocab-<n>`. Every relationship carries its predicate and the vocabulary version it was written under. Adding a predicate is a minor bump; renaming or removing one is a major bump with a recorded mapping. Each predicate declares direction, source and target classes and an inverse where one exists, and the library checks the class constraints on insert.

**Extension.** Namespaced predicates such as `x-archive:MEMBER_OF` are accepted without a registry change and are never promoted into the core vocabulary without a bump and a mapping. This is Reflective Rust's lane rule applied to predicates: crossing from an extension into the core requires explicit reification.

**Interchange.** Because the envelope rejects unknown fields, an unregistered predicate on import is preserved as a value, `predicate: "x-unknown:…"` with its original vocabulary version, excluded from materialization until mapped, and never dropped.

### D4. Signatures: hash-chained locally, signed at the boundary

**Decision.** The local operation log stores unsigned envelopes; each event carries the canonical hash of its predecessor, so local integrity is a hash chain. Signing with the owner's key happens at every boundary where an envelope leaves the device: export, share, federation, or a consumer on another machine. Events that arrive from a different actor, such as an assistant or a host, must be signed by that actor's key before they enter the log, because the log is the trust boundary for foreign assertions.

**Why.** RSP itself separates `DescriptorEnvelope` from `AuthenticatedEnvelope`, and signing every local write with a key that sits beside the database proves nothing the hash chain does not. The decision is a design judgment, not a verified constraint.

### D5. SSS: a minimal descriptor meeting S0 and S2 only

**Evidence.** The SSS corpus defines five conformance levels. S0 is structural, S2 is the access and read-only boundary, S1 adds deterministic resolution machinery, S3 and S4 require multiple expression implementations and domains. Its schema and conformance folders are empty.

**Decision.** The Stage 2 surface descriptor is a subset of `ResolvedSurface`: `protocolVersion`, `resolutionId`, `sourceSnapshot`, `accessProjection`, `groups` with `SurfaceMembership.basis`, `operations` with the five exposure states, an `explanation` limited to included and excluded traces, and `fingerprint`. Eligibility keeps all five states, `eligible`, `ineligible`, `unknown`, `withheld`, `unsupported`. The ten mandatory negative tests from the SSS conformance document are written as fixtures. The result is labeled "SSS-inspired, S0 and S2", never "SSS conformant". Adopting the full normative candidate waits until SSS has a resolver of its own.

### D6. Access model: audience sets, projected to levels only at interchange

**Decision.** Every object carries an owner and an audience set, the model a household assistant needs, because household audiences are not totally ordered: kids and adults are not nested. RSP's ordered access levels, `public < member < owner < administrator`, are a disclosure projection, so they are applied only when an envelope leaves the device: owner-only maps to `owner`, any household audience to `member`, external to `public`. Stage 1 has one policy record whose only authority is `owner-local`; every mutation names an authority reference even when that is the only value. Federation later adds signed share edges as authority grants without changing the schema. Derived objects inherit the most private audience among their sources.

### D7. Presentation identity: session-scoped, re-bound through the member

**Evidence.** FSKit's `FSItemID` is a per-volume 64-bit value the extension assigns; the header says to provide a unique identifier like an inode number. A read-only archive-mount extension that assigns these from a counter starting after the root directory yields ids that are not stable across mounts. `FSEntityIdentifier` is a UUID per container or volume with an optional eight-byte qualifier; such an extension may use one constant volume UUID.

**Decision.** A presentation gets a session-scoped identifier, `(volume entity UUID, item id, mount epoch)`, with a validity window. Re-binding on remount is done through the durable identity of the contained member, keyed by `(container artifact revision, member path, member size and checksum)`, never through the item id. The same rule applies to DiskImageKit attachments, package contents and remote volumes. The two identities are kept apart because they answer different questions: "what is this" and "where can I read it right now".

### D8. macOS 27 SDK: verified availability

| Track | Verified in the installed SDK | Consequence |
|---|---|---|
| App Intents indexing | `IndexedEntity: AppEntity` with `attributeSet: CSSearchableItemAttributeSet`, available macOS 15; `hideInSpotlight` from 15.4 | A `Thing` can be a Spotlight-indexed entity; the non-disclosure test is required because the attribute set is what Spotlight sees |
| Core Spotlight | Present | Index artifacts and things with access checked before indexing |
| Foundation Models | Present, with macOS 26 and macOS 27 API families; `LanguageModel`, `Tool`, `Generable`, `Attachment` protocols and types | Model-assisted candidate proposals are possible on-device; outputs remain proposals with `derived` provenance |
| Vision | Present | Serial-number and document extraction as observations |
| FSKit | Present; `FSEntityIdentifier`, `FSItemID` as above | Archive mounts and any future managed mount follow D7 |
| DiskImageKit | Present; ASIF and RAW creation configurations, `OpenConfiguration`, format and block-size enums; no attach API seen in the module interface | Image creation and open are verifiable; attach semantics remain to be checked |
| "Evaluations" | No such framework in the SDK | Dropped as a track; evaluation is this project's own fixture suite |

### D9. Contradiction: polarity on assertions, four materialization states

**Decision.** Every assertion has a polarity. A relationship's materialized state is derived from accepted assertions only:

| State | Rule |
|---|---|
| `PRESENT` | At least one accepted `SUPPORTS`, no accepted `DISPUTES` |
| `DISPUTED` | At least one accepted `SUPPORTS` and at least one accepted `DISPUTES` |
| `ABSENT` | At least one accepted `DISPUTES`, no accepted `SUPPORTS` (this is known absence) |
| `UNKNOWN` | No accepted assertion; proposals may exist |

`DISPUTED` never resolves on its own. It routes to the person, who records a resolution event; that event is evidence in its own right, which is the formal model's rule that authority may establish a judgment-bearing property. A low-access projection may collapse `DISPUTED` to `withheld` only when the projection contract declares it. Materialized views always carry the state, not a boolean.

### Still open

1. Attach semantics in DiskImageKit, and whether attached images surface through FSKit or a system mount.
2. Whether `sc-event-0.1` reuses the RSP crate as a dependency for its envelope types or vendors the rules; the conformance vector makes either safe.
3. The exact fixture set for cross-volume moves on removable, network and iCloud volumes, since document identifiers behave differently on non-APFS volumes and the header does not promise behavior there.

## 14. Architectural statement

**Semantic Core owns stable identities, typed relationships, evidence-linked assertions, and their event history. Reflective Rust informs portable envelope discipline. EQP informs what evidence justifies, without granting acceptance. FCI informs epistemic distinctions. SSS selects an authorized, inspectable semantic surface; SES preserves meaning across expressions. ICP governs consequential actions. Finder and other applications present and act on authorized results. Fundamental Engine is an optional computational and spatial consumer.**

The first product proof is deliberately modest: a relationship survives a justified file move, remains inspectable through a native macOS entry point, does not leak through unauthorized views, and can be retracted without rewriting history.

---

**Changes from v0.2.1:** section 13 rewritten from open decisions to decisions, each with its evidence; assertion polarity added to the ontology; the vocabulary sentence corrected to preserve unregistered predicates as values; three items left open with reasons.

**Changes from v0.2.0:** relationship and assertion identities separated; presentation added for mounted and virtual locations; trust class added and separated from provenance, integrity, and authority; three decision vocabularies tabulated with the Distance from Gray v1.0 model cited by path; RSP protocol facts recorded; single-owner default stated; container and mount treatment added; reserved names for episode, time facets, and known absence; fixtures extended; open decision 7 closed and replaced.

**Source basis:** the protocol corpora named in section 6 and the architectural discussion recorded in the project's design history. Proposed implementation choices are labeled as such and are **not** claims that the referenced protocols or macOS SDK integrations have been implemented or verified.
