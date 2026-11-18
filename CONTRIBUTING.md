# Contributing to Semantic Core

Semantic Core is a portable identity and relationship substrate: a Rust workspace, a C ABI and a Swift package. This guide covers setup, the checks to run and the conventions the repository uses.

Read [`AUTHORITY.md`](AUTHORITY.md) first. It sets the order in which the intent, the definition of record ([`docs/semantic-core-definition-v0.2.2.md`](docs/semantic-core-definition-v0.2.2.md)), the fixtures and the code take precedence. The stage plan is in [`docs/ROADMAP.md`](docs/ROADMAP.md).

## Setup

You need Rust 1.80 or later. The Swift package and FFI checks need macOS with Swift and the `aarch64-apple-darwin` and `x86_64-apple-darwin` Rust targets.

```sh
git clone https://github.com/zachshallbetter/semantic-core.git
cd semantic-core
cargo build --workspace
```

## Checks

CI runs these (`.github/workflows/ci.yml`). Run the Rust checks for any change, and the Swift checks when you touch `crates/sc-ffi`, `swift/` or `scripts/`:

```sh
cargo build --workspace
cargo test --workspace
scripts/build-ffi.sh
swift test --package-path swift/SemanticCore
scripts/package-swift.sh
scripts/test-swift-distribution.sh
```

## Conventions

- **The definition controls.** Code that disagrees with the definition is a defect or a proposal to change it, never a silent redefinition. A change to the definition is a new version with a changelog entry.
- **Fixtures are the proof.** Behavior changes come with a fixture under `fixtures/` or a test next to the code. Pass the golden vectors in Rust and in Swift.
- **Status honesty.** Where prose and code disagree about what is implemented, the weaker claim governs. Say implemented, tested or not started, and nothing stronger than the fixtures prove.
- **Identity is opaque.** Object identity is never derived from a path, a hash, a device or a signing key.
- **History is append-only.** Undo is a compensating event.
- **Lockfile.** Commit `Cargo.lock` with dependency changes; the FFI build enforces it.
- **Commits.** Short imperative subjects, with a conventional prefix where it fits (`ci:`, `docs:`, `fix:`).
- **Changelog.** Note user-visible changes under `## Unreleased` in `CHANGELOG.md`.

## Pull requests

Describe what changed and why, list the checks you ran, and keep the diff to one concern. Use the pull request template.

Report vulnerabilities through [SECURITY.md](SECURITY.md), not a public issue.
