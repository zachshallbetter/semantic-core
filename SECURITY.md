# Security Policy

## Reporting a vulnerability

Report security issues privately through GitHub private vulnerability reporting:
<https://github.com/zachshallbetter/semantic-core/security/advisories/new>.
Do not open a public issue for a vulnerability.

Include a description of the issue and its impact, steps to reproduce (a minimal example is ideal), and the affected version or commit.

This is a solo-maintained project. Handling is best-effort with no SLA: there is no guaranteed acknowledgement or fix time, and the maintainer will respond when next active.

## Scope

Semantic Core is a local-first identity and relationship store: a Rust core, a CLI (`sc`), a C ABI (`sc-ffi`) and a Swift package over it. Its design depends on signed, hash-chained events and on access being enforced before anything is revealed. In scope:

- Envelope verification: canonical JSON (RFC 8785) parsing in Rust and in Swift, SHA-256 identity, Ed25519 signatures, and freshness checks.
- Access control: any way a restricted viewer can learn that withheld data exists, or read it, through inspection, traversal, counts, search, export or the Spotlight entity projection.
- Import and export: accepting events from another store, including foreign-owner events and origin aliasing.
- The SQLite materialization and the append-only event log: integrity, ordering and replay.
- The C ABI in `crates/sc-ffi`: memory safety and ownership across the boundary.

Reports about incorrect resolution of moved or copied files are ordinary bugs unless they disclose data or break a signature or chain check.

## Supported versions

Only the latest tagged release and `main` receive fixes.
