# Project Intent

**Project ID:** `semantic-core`
**Project Name:** `Semantic Core`
**Status:** Active (Stage 0 verification complete; Stage 1 implementation starting 2026-10-10)
**Bootstrap:** Formal Project Bootstrap 0.6.0 templates

## Originating intent

A person's files, documents and the real things they describe have no durable identity or relationships on their devices. Finder knows locations, Spotlight indexes properties, and every assistant re-extracts the same facts from the same documents. Semantic Core changes that condition for one person first: stable identities for things, artifacts, revisions and instances; typed relationships with evidence, provenance and authority; and a reversible history, all in a local library that no application, renderer or model owns.

- **What condition should change.** A justified relationship between two files survives a rename or move; a copy is distinguished from a move; a missing file never deletes the thing it belonged to; every relationship can explain why it exists.
- **For whom.** One person on their own macOS and iOS devices. Household federation, Jellybean, titan, ASE and Fundamental Engine are later consumers, not the first user.
- **Why the project exists.** The substrate should outlive the assistants built on it. Identity and relationships belong to the person, not to whichever tool is current.
- **What counts.** The Stage 1 acceptance list in the definition, section 11, plus the three additions in the build brief, green in CI.
- **What does not count.** A demo that needs a model, a server, a File Provider domain or a Finder modification to work.

## Desired state

```text
CURRENT CONDITION
  files have paths; relationships live in people's heads or in one app's database
      ↓
FORMAL / OPERATIONAL WORK
  identities, relationships, assertions, observations, event log, resolver, envelope, vocabulary;
  executable fixtures for every claim
      ↓
QUALIFIED TARGET CONDITION
  a relationship survives a justified move, is inspectable from Finder, does not leak through
  unauthorized views, and can be retracted without rewriting history
```

## Constraints

```text
ownership / licensing   public repository, MIT OR Apache-2.0; commits authored by the owner
privacy / data          single local owner, private by default, no implicit sharing; fixtures use fictional names only
security                access enforced before traversal, counts, search, export and surfaces; file contents and
                        model outputs are untrusted data; secrets never on argv
cost                    no mandatory account, server, cloud sync or model; the Rust core builds and tests on Linux CI, the Swift package on macOS
platform                macOS 27 SDK verified; Mac indexes, iOS reads; Swift package over a Rust core
human authority         acceptance of an assertion requires separate authority; qualification cannot self-promote
protocols               RSP envelope discipline, EQP qualification, FCI distinctions, SSS/SES boundaries, ICP for
                        consequential actions: borrowed as definitions, never as runtime dependencies
```

## Non-goals

Replacing Finder's window layout or injecting panels into it. Replacing iCloud Drive or any blob store on day one. A complete EQP engine, ICP runtime, FCI implementation, SSS resolver or SES expression engine as prerequisites. Automatic merging of entities on a hash match or a model prediction. Multi-device synchronization, household federation or unattended consequential actions in Stage 1. Windows.

## Success questions

At maturity, canonical state should answer:

```text
Which object is this?                        identity resolution: RESOLVED, AMBIGUOUS, UNRESOLVED
Why does this relationship exist?            assertion, polarity, evidence, actor, authority, history
What does the evidence justify?              qualification, kept separate from acceptance
Who may see this, and what is withheld?      access projection before every read
What changed, and how do I undo it?          append-only events, compensating undo
What is unresolved?                          open decisions in the definition, section 13
```

## Termination / dormancy

Dormant if Stage 1 is not green within its first bounded effort and no consumer needs it. Superseded if an Apple framework provides durable cross-location identity and typed relationships natively. Transfers to routine operation when Jellybean or titan reads it in production.
