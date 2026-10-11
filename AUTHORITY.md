# Authority and Conflict Resolution

**Status:** Repository doctrine, adapted from Formal Project Bootstrap 0.6.0
**Applies to:** this repository

## Resolution order

When artifacts in this repository disagree, the first applicable item controls:

1. `AUTHORITY.md` and decisions recorded by the owner;
2. `PROJECT_INTENT.md`;
3. the definition of record, `docs/semantic-core-definition-v0.2.2.md`, and the build brief;
4. conformance fixtures and golden vectors under `fixtures/`;
5. crate code and tests;
6. generated artifacts, lockfiles, CI output and the thought-experiment record (history, not authority).

External protocols this project borrows from (Reflective Rust RSP, EQP, FCI, SSS, SES, ICP, Distance from Gray) keep their own authority. This repository records where it departs from them and never rewrites them.

## Conflict rule

A lower layer may expose a contradiction but cannot resolve it by changing the meaning of a higher layer. Code that disagrees with the definition is a defect in the code or a proposal to change the definition, never a silent redefinition. Fixtures that pass while the definition says they should fail are a defect in the fixtures.

## Status honesty

Where prose and code disagree about what is implemented, the weaker claim governs. The README's status line, crate doc comments and the build brief's checklist must say implemented, tested or not started, and nothing stronger than the fixtures prove.

## Evidence

Evidence is orthogonal to precedence. A fixture that fails may challenge the definition and trigger a revision, but evidence does not mutate the definition it evaluates. Revisions are new versions of the definition with a changelog, never edits in place.

## Authority over changes

- The owner decides acceptance of definition revisions, protocol departures and anything that widens scope.
- Changes arrive as pull requests that follow the build brief's task order and are merged by the owner after review.
- Commits are authored by the owner.
