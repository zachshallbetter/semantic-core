//! Semantic Core: identities, relationships, assertions, observations and
//! their event history in one local SQLite library.
//!
//! Implemented and tested in Stage 1 (see `tests/` and `fixtures/`):
//!
//! - Entities: things, artifacts, revisions, instances, presentations; each
//!   carries an owner and an audience set (decision D6).
//! - A hash-chained, append-only event log of `sc-event-0.1` envelopes; every
//!   event carries the canonical hash of its predecessor. `verify_chain` and
//!   `replay` rebuild the materialized tables from the log.
//! - Relationships are distinct from assertions. Assertions carry polarity
//!   (`SUPPORTS` | `DISPUTES`), evidence and a lifecycle `proposed ->
//!   accepted -> retracted`. State `PRESENT | DISPUTED | ABSENT | UNKNOWN` is
//!   materialized from accepted assertions only (decision D9). Acceptance
//!   requires the policy authority; qualification never promotes.
//! - Retraction appends a compensating event and preserves evidence.
//! - Resolver with `RESOLVED | AMBIGUOUS | UNRESOLVED` following decision D2's
//!   rules; each resolution records the rule and its evidence.
//! - Access checks before every get, traversal, count, search, history and
//!   export. Absent and withheld identifiers are reported identically.
//! - Container members as presentations with session-scoped locators,
//!   re-bound through the member key (decision D7); extraction creates a
//!   distinct derived instance.
//! - Signed interchange: export signs with the owner's key; import verifies,
//!   keeps referential integrity and is idempotent.
//!
//! Not implemented: EQP qualification beyond recording a result, SSS surface
//! descriptors, ICP receipts, multi-device policy, audience federation,
//! anything that needs macOS APIs. Entities are owned by the library owner;
//! a second owner inside one library is not modelled.

pub mod containers;
pub mod error;
pub mod events;
pub mod graph;
pub mod interchange;
pub mod model;
pub mod resolver;
pub mod schema;
pub mod store;

pub use containers::MemberSpec;
pub use error::{Error, Result};
pub use events::{Change, Event, EventEnvelope, EventRecord, EVENT_SCHEMA};
pub use graph::{Direction, Inspection, RelationshipView, SearchHit, Why};
pub use interchange::{ImportReport, Interchange, INTERCHANGE_FORMAT};
pub use model::*;
pub use resolver::Seen;
pub use store::{Kind, Store, OWNER_LOCAL};

pub use sc_envelope;
pub use sc_vocab;
