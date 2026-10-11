//! The store: SQLite connection, policy, the hash-chained event log, the
//! materializer that applies events to tables, entity creation and the access
//! checks every read goes through.

use crate::error::{Error, Result};
use crate::events::*;
use crate::model::*;
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use sc_envelope::{AccessLevel, Provenance, ProvenanceKind};
use sc_vocab::EntityClass;
use serde::de::DeserializeOwned;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::Path;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

/// The only authority Stage 1 knows (decision D6).
pub const OWNER_LOCAL: &str = "owner-local";

pub type Clock = Box<dyn Fn() -> OffsetDateTime + Send>;

pub struct Store {
    pub(crate) conn: Connection,
    pub(crate) owner: String,
    pub(crate) authority: String,
    clock: Clock,
}

impl std::fmt::Debug for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Store").field("owner", &self.owner).finish()
    }
}

/// Which class an identifier belongs to, including the non-entity records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Entity(EntityClass),
    Relationship,
    Assertion,
    Observation,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Entity(class) => class.as_str(),
            Self::Relationship => "relationship",
            Self::Assertion => "assertion",
            Self::Observation => "observation",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        match text {
            "relationship" => Some(Self::Relationship),
            "assertion" => Some(Self::Assertion),
            "observation" => Some(Self::Observation),
            other => EntityClass::parse(other).map(Self::Entity),
        }
    }
}

impl Store {
    /// Open an existing library.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if !path.exists() {
            return Err(Error::NotFound(format!("library {}", path.display())));
        }
        let conn = Connection::open(path)?;
        Self::from_connection(conn, None)
    }

    /// Open a library, establishing the single-owner policy when the file is new.
    pub fn open_or_create(path: impl AsRef<Path>, owner: &str) -> Result<Self> {
        let conn = Connection::open(path)?;
        Self::from_connection(conn, Some(owner))
    }

    pub fn in_memory(owner: &str) -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::from_connection(conn, Some(owner))
    }

    fn from_connection(conn: Connection, owner: Option<&str>) -> Result<Self> {
        conn.execute_batch(crate::schema::SCHEMA)?;
        conn.execute_batch(crate::schema::PREVIOUS_INDEX).map_err(|e| {
            Error::Integrity(format!(
                "the event log already holds a fork (two events name the same predecessor), so the one-successor index cannot be created; repair the log before opening this library ({e})"
            ))
        })?;
        conn.execute(
            "INSERT OR IGNORE INTO library_secret (id, salt) VALUES ('default', ?1)",
            params![fresh_salt()],
        )?;
        {
            let salt = salt_in(&conn)?;
            let mut missing = conn.prepare(
                "SELECT hash FROM events WHERE hash NOT IN (SELECT hash FROM event_aliases)
                 UNION SELECT origin FROM events WHERE origin IS NOT NULL AND origin NOT IN (SELECT hash FROM event_aliases)",
            )?;
            let hashes: Vec<String> = missing.query_map([], |row| row.get(0))?.collect::<std::result::Result<_, _>>()?;
            for hash in hashes {
                conn.execute(
                    "INSERT OR IGNORE INTO event_aliases (hash, alias) VALUES (?1, ?2)",
                    params![hash, alias_for(&salt, &hash)],
                )?;
            }
        }
        let existing: Option<(String, String)> = conn
            .query_row("SELECT authority, owner FROM policy WHERE id = 'default'", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .optional()?;
        let mut store = Self {
            conn,
            owner: String::new(),
            authority: OWNER_LOCAL.into(),
            clock: Box::new(OffsetDateTime::now_utc),
        };
        match (existing, owner) {
            (Some((authority, existing_owner)), requested) => {
                if let Some(requested) = requested {
                    if requested != existing_owner {
                        return Err(Error::Unauthorized(format!(
                            "{requested:?} is not the owner of this library"
                        )));
                    }
                }
                store.owner = existing_owner;
                store.authority = authority;
            }
            (None, Some(owner)) => {
                store.owner = owner.to_string();
                let principal = Principal::solo(owner);
                store.commit(
                    &principal,
                    Change::PolicyEstablished {
                        authority: OWNER_LOCAL.into(),
                        owner: owner.to_string(),
                    },
                    ProvenanceKind::Declared,
                    None,
                    &BTreeSet::new(),
                )?;
            }
            (None, None) => return Err(Error::Integrity("library has no policy record".into())),
        }
        Ok(store)
    }

    /// Replace the clock; tests use a fixed one.
    pub fn set_clock(&mut self, clock: impl Fn() -> OffsetDateTime + Send + 'static) {
        self.clock = Box::new(clock);
    }

    pub fn owner(&self) -> &str {
        &self.owner
    }

    pub fn authority(&self) -> &str {
        &self.authority
    }

    pub fn owner_principal(&self) -> Principal {
        Principal::solo(self.owner.clone())
    }

    pub(crate) fn now_time(&self) -> OffsetDateTime {
        (self.clock)()
    }

    pub(crate) fn now(&self) -> String {
        self.now_time().format(&Rfc3339).expect("rfc3339")
    }

    pub(crate) fn new_id() -> String {
        ulid::Ulid::new().to_string()
    }

    pub(crate) fn access_for(&self, audience: BTreeSet<String>) -> Access {
        Access {
            owner: self.owner.clone(),
            audience,
        }
    }

    /// Only the owner mutates under the Stage 1 policy.
    pub(crate) fn require_owner(&self, principal: &Principal) -> Result<()> {
        if principal.actor == self.owner {
            Ok(())
        } else {
            Err(Error::Unauthorized(format!(
                "{} is not the owner of this library",
                principal.actor
            )))
        }
    }

    // ----------------------------------------------------------------- log

    /// Append one change to the log and apply it, in a single transaction.
    pub(crate) fn commit(
        &mut self,
        principal: &Principal,
        change: Change,
        kind: ProvenanceKind,
        window: Option<(String, String)>,
        audience: &BTreeSet<String>,
    ) -> Result<Event> {
        self.commit_with_origin(principal, change, kind, window, audience, None)
    }

    pub(crate) fn commit_with_origin(
        &mut self,
        principal: &Principal,
        change: Change,
        kind: ProvenanceKind,
        window: Option<(String, String)>,
        audience: &BTreeSet<String>,
        origin: Option<String>,
    ) -> Result<Event> {
        let at = self.now();
        // The write lock is taken before `previous` is read, so two writers
        // cannot both chain to the same predecessor; UNIQUE(previous) backs
        // that up at the schema.
        let tx = self.conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let previous = last_hash_in(&tx)?;
        let record = EventRecord {
            previous,
            actor: principal.actor.clone(),
            authority: self.authority.clone(),
            at: at.clone(),
            origin,
            imported_at: None,
            source_owner: None,
            change,
        };
        let envelope = Self::envelope_for(record, kind, window, audience)?;
        let event = Self::insert_and_apply(&tx, envelope)?;
        tx.commit()?;
        Ok(event)
    }

    pub(crate) fn envelope_for(
        record: EventRecord,
        kind: ProvenanceKind,
        window: Option<(String, String)>,
        audience: &BTreeSet<String>,
    ) -> Result<EventEnvelope> {
        Self::envelope_with_access(record, kind, window, project_audience(audience))
    }

    pub(crate) fn envelope_with_access(
        record: EventRecord,
        kind: ProvenanceKind,
        window: Option<(String, String)>,
        minimum_access: AccessLevel,
    ) -> Result<EventEnvelope> {
        let (class, id) = record.change.subject();
        let subject_id = format!("sc:{}:{}", class.prefix(), id);
        let source = format!("actor:{}", record.actor);
        let source_hash = sc_envelope::hash_bytes(source.as_bytes());
        let provenance = match (kind, window) {
            (ProvenanceKind::Observed, Some((observed_at, expires_at))) => {
                Provenance::observed(record.authority.clone(), source, source_hash, observed_at, expires_at)
            }
            (ProvenanceKind::Observed, None) => {
                return Err(Error::Integrity("observed events need a freshness window".into()))
            }
            (kind, _) => Provenance::new(kind, record.authority.clone(), source, source_hash),
        };
        Ok(EventEnvelope::new(subject_id, EVENT_SCHEMA, provenance, minimum_access, record)?)
    }

    pub(crate) fn insert_and_apply(tx: &Transaction<'_>, envelope: EventEnvelope) -> Result<Event> {
        let hash = envelope.canonical_hash()?;
        let record = &envelope.body;
        let subject = envelope.subject_id.clone();
        tx.execute(
            "INSERT INTO events (hash, previous, origin, subject, at, envelope) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                hash,
                record.previous,
                record.origin,
                subject,
                record.at,
                serde_json::to_string(&envelope)?
            ],
        )?;
        let seq = tx.last_insert_rowid();
        let salt = salt_in(tx)?;
        tx.execute(
            "INSERT OR IGNORE INTO event_aliases (hash, alias) VALUES (?1, ?2)",
            params![hash, alias_for(&salt, &hash)],
        )?;
        if let Some(origin) = &record.origin {
            tx.execute(
                "INSERT OR IGNORE INTO event_aliases (hash, alias) VALUES (?1, ?2)",
                params![origin, alias_for(&salt, origin)],
            )?;
        }
        Self::apply(tx, &record.change, &record.at)?;
        Ok(Event { seq, hash, envelope })
    }

    /// An event as a principal may see it. The owner sees the log as stored;
    /// anyone else sees positions renumbered within their own view and no
    /// `previous` link, so gaps left by withheld events do not show.
    pub(crate) fn project_event(&self, principal: &Principal, mut event: Event, position: usize) -> Result<Event> {
        if principal.actor != self.owner {
            event.seq = position as i64;
            event.envelope.body.previous = None;
            event.hash = self.viewer_event_id(principal, &event.hash)?;
            if let Some(origin) = &mut event.envelope.body.origin {
                *origin = self.viewer_event_id(principal, origin)?;
            }
            if let Change::AssertionRetracted { compensates, .. } = &mut event.envelope.body.change {
                *compensates = self.viewer_event_id(principal, compensates)?;
            }
        }
        Ok(event)
    }

    /// The identifier a restricted viewer sees for an event: keyed by the
    /// library secret and the viewer, so it cannot be recomputed from the
    /// envelope and does not correlate across viewers.
    pub fn viewer_event_id(&self, principal: &Principal, hash: &str) -> Result<String> {
        let salt = salt_in(&self.conn)?;
        let mut hasher = Sha256::new();
        hasher.update(salt.as_bytes());
        hasher.update([0]);
        hasher.update(principal.actor.as_bytes());
        hasher.update([0]);
        hasher.update(hash.as_bytes());
        Ok(format!("view:{:x}", hasher.finalize()))
    }

    /// The per-library alias of a stored hash, as used for `origin` in a
    /// restricted export.
    pub(crate) fn alias_of(&self, hash: &str) -> Result<String> {
        Ok(alias_for(&salt_in(&self.conn)?, hash))
    }

    /// Every event in order. Owner only: the raw log is the trust boundary.
    pub fn events(&self, principal: &Principal) -> Result<Vec<Event>> {
        self.require_owner(principal)?;
        self.all_events()
    }

    pub(crate) fn all_events(&self) -> Result<Vec<Event>> {
        let mut statement = self.conn.prepare("SELECT seq, hash, envelope FROM events ORDER BY seq")?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
        })?;
        let mut events = Vec::new();
        for row in rows {
            let (seq, hash, envelope) = row?;
            let envelope: EventEnvelope = serde_json::from_str(&envelope)?;
            events.push(Event { seq, hash, envelope });
        }
        Ok(events)
    }

    /// Walk the log, recomputing every hash and checking each `previous`
    /// link. Returns the number of events when the chain is intact. Owner
    /// only: the count and the gaps are the log's shape, which a restricted
    /// principal may not learn.
    pub fn verify_chain_as(&self, principal: &Principal) -> Result<usize> {
        self.require_owner(principal)?;
        self.verify_chain()
    }

    /// `verify_chain_as` for the owner; used by replay and by tests that
    /// hold the library itself.
    pub fn verify_chain(&self) -> Result<usize> {
        let events = self.all_events()?;
        let mut previous: Option<String> = None;
        for event in &events {
            let recomputed = event.envelope.canonical_hash()?;
            if recomputed != event.hash {
                return Err(Error::ChainBroken {
                    seq: event.seq,
                    reason: format!("stored hash {} but canonical bytes hash to {}", event.hash, recomputed),
                });
            }
            if event.envelope.body.previous != previous {
                return Err(Error::ChainBroken {
                    seq: event.seq,
                    reason: format!(
                        "previous is {:?} but the preceding event is {:?}",
                        event.envelope.body.previous, previous
                    ),
                });
            }
            previous = Some(event.hash.clone());
        }
        Ok(events.len())
    }

    /// Rebuild every materialized table from the log, verifying the chain
    /// first. Returns the number of events replayed.
    pub fn replay(&mut self) -> Result<usize> {
        self.verify_chain()?;
        let events = self.all_events()?;
        let tx = self.conn.transaction()?;
        for table in crate::schema::MATERIALIZED_TABLES {
            tx.execute(&format!("DELETE FROM {table}"), [])?;
        }
        for event in &events {
            Self::apply(&tx, &event.envelope.body.change, &event.envelope.body.at)?;
        }
        tx.commit()?;
        Ok(events.len())
    }

    // --------------------------------------------------------- materializer

    pub(crate) fn apply(tx: &Transaction<'_>, change: &Change, at: &str) -> Result<()> {
        match change {
            Change::PolicyEstablished { authority, owner } => {
                tx.execute(
                    "INSERT OR REPLACE INTO policy (id, authority, owner) VALUES ('default', ?1, ?2)",
                    params![authority, owner],
                )?;
            }
            Change::ThingCreated { thing } => {
                insert_entity(tx, &thing.id, Kind::Entity(EntityClass::Thing), &thing.label, &thing.access)?;
                tx.execute("INSERT INTO things (id, doc) VALUES (?1, ?2)", params![thing.id, json(thing)?])?;
            }
            Change::ArtifactCreated { artifact } => {
                insert_entity(
                    tx,
                    &artifact.id,
                    Kind::Entity(EntityClass::Artifact),
                    &artifact.label,
                    &artifact.access,
                )?;
                tx.execute(
                    "INSERT INTO artifacts (id, doc) VALUES (?1, ?2)",
                    params![artifact.id, json(artifact)?],
                )?;
            }
            Change::RevisionCreated { revision } => {
                require_row(tx, "artifacts", &revision.artifact_id)?;
                insert_entity(tx, &revision.id, Kind::Entity(EntityClass::Revision), "", &revision.access)?;
                tx.execute(
                    "INSERT INTO revisions (id, artifact_id, content_hash, doc) VALUES (?1, ?2, ?3, ?4)",
                    params![revision.id, revision.artifact_id, revision.content_hash, json(revision)?],
                )?;
            }
            Change::InstanceRegistered { instance } => {
                require_row(tx, "artifacts", &instance.artifact_id)?;
                require_row(tx, "revisions", &instance.revision_id)?;
                insert_entity(
                    tx,
                    &instance.id,
                    Kind::Entity(EntityClass::Instance),
                    &instance.location.path,
                    &instance.access,
                )?;
                write_instance(tx, instance, true)?;
            }
            Change::InstanceUpdated {
                instance_id,
                location,
                revision_id,
                status,
                ..
            } => {
                let mut instance: Instance = read_doc(tx, "instances", instance_id)?;
                if let Some(location) = location {
                    instance.location = location.clone();
                }
                if let Some(revision_id) = revision_id {
                    require_row(tx, "revisions", revision_id)?;
                    instance.revision_id = revision_id.clone();
                }
                instance.status = *status;
                write_instance(tx, &instance, false)?;
                tx.execute(
                    "UPDATE entities SET label = ?2 WHERE id = ?1",
                    params![instance.id, instance.location.path],
                )?;
            }
            Change::ObservationRecorded { observation, resolution } => {
                // Freshness binds again on replay and import: a resolution
                // may not rest on an observation whose window had not opened,
                // or had closed, when the event was recorded.
                if resolution.outcome == Outcome::Resolved {
                    let observed = sc_envelope::parse_timestamp("observation.observed_at", Some(&observation.observed_at))?;
                    let expires = sc_envelope::parse_timestamp("observation.expires_at", Some(&observation.expires_at))?;
                    let recorded = sc_envelope::parse_timestamp("body.at", Some(at))?;
                    if recorded < observed {
                        return Err(Error::Integrity(format!(
                            "observation {} is not valid before {} but drove resolution {} recorded at {at}",
                            observation.id, observation.observed_at, resolution.id
                        )));
                    }
                    if recorded >= expires {
                        return Err(Error::Integrity(format!(
                            "observation {} expired at {} but drove resolution {} recorded at {at}",
                            observation.id, observation.expires_at, resolution.id
                        )));
                    }
                }
                tx.execute(
                    "INSERT INTO observations (id, doc) VALUES (?1, ?2)",
                    params![observation.id, json(observation)?],
                )?;
                tx.execute(
                    "INSERT INTO resolutions (id, observation_id, instance_id, doc) VALUES (?1, ?2, ?3, ?4)",
                    params![resolution.id, resolution.observation_id, resolution.instance_id, json(resolution)?],
                )?;
                insert_entity(
                    tx,
                    &observation.id,
                    Kind::Observation,
                    "",
                    &Access {
                        owner: policy_owner(tx)?,
                        audience: BTreeSet::new(),
                    },
                )?;
            }
            Change::PresentationMounted { presentation } => {
                require_row(tx, "revisions", &presentation.member.container_revision_id)?;
                require_row(tx, "artifacts", &presentation.member_artifact_id)?;
                require_row(tx, "revisions", &presentation.member_revision_id)?;
                insert_entity(
                    tx,
                    &presentation.id,
                    Kind::Entity(EntityClass::Presentation),
                    &presentation.member.member_path,
                    &presentation.access,
                )?;
                write_presentation(tx, presentation, true)?;
            }
            Change::PresentationRebound {
                presentation_id,
                locator,
                valid_from,
            } => {
                let mut presentation: Presentation = read_doc(tx, "presentations", presentation_id)?;
                presentation.locator = locator.clone();
                presentation.status = PresentationStatus::Available;
                presentation.valid_from = valid_from.clone();
                presentation.valid_until = None;
                write_presentation(tx, &presentation, false)?;
            }
            Change::PresentationUnavailable {
                presentation_id,
                valid_until,
            } => {
                let mut presentation: Presentation = read_doc(tx, "presentations", presentation_id)?;
                presentation.status = PresentationStatus::Unavailable;
                presentation.valid_until = Some(valid_until.clone());
                write_presentation(tx, &presentation, false)?;
            }
            Change::RelationshipDeclared { relationship } => {
                require_entity(tx, &relationship.source_id)?;
                require_entity(tx, &relationship.target_id)?;
                // The flag is derived from the predicate, never trusted from
                // the record: an imported file cannot promote an unknown one.
                let predicate = sc_vocab::Predicate::parse(&relationship.predicate)
                    .map_err(|e| Error::Integrity(format!("relationship {}: {e}", relationship.id)))?;
                let mut relationship = relationship.clone();
                relationship.materializes = predicate.materializes();
                let relationship = &relationship;
                insert_entity(
                    tx,
                    &relationship.id,
                    Kind::Relationship,
                    &relationship.predicate,
                    &relationship.access,
                )?;
                tx.execute(
                    "INSERT INTO relationships (id, predicate, source_id, target_id, materializes, state, doc)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        relationship.id,
                        relationship.predicate,
                        relationship.source_id,
                        relationship.target_id,
                        relationship.materializes as i64,
                        RelationshipState::Unknown.as_str(),
                        json(relationship)?
                    ],
                )?;
            }
            Change::AssertionProposed { assertion } => {
                require_row(tx, "relationships", &assertion.relationship_id)?;
                insert_entity(tx, &assertion.id, Kind::Assertion, "", &assertion.access)?;
                write_assertion(tx, assertion, true)?;
                rematerialize(tx, &assertion.relationship_id)?;
            }
            Change::QualificationRecorded {
                assertion_id,
                qualification,
            } => {
                let mut assertion: Assertion = read_doc(tx, "assertions", assertion_id)?;
                assertion.qualification = Some(*qualification);
                write_assertion(tx, &assertion, false)?;
            }
            Change::AssertionAccepted {
                assertion_id,
                authority,
            } => {
                let mut assertion: Assertion = read_doc(tx, "assertions", assertion_id)?;
                assertion.status = AssertionStatus::Accepted;
                assertion.accepted_by = Some(authority.clone());
                write_assertion(tx, &assertion, false)?;
                rematerialize(tx, &assertion.relationship_id)?;
            }
            Change::AssertionRetracted { assertion_id, .. } => {
                let mut assertion: Assertion = read_doc(tx, "assertions", assertion_id)?;
                assertion.status = AssertionStatus::Retracted;
                write_assertion(tx, &assertion, false)?;
                rematerialize(tx, &assertion.relationship_id)?;
            }
            Change::AudienceChanged { entity_id, audience } => {
                require_entity(tx, entity_id)?;
                tx.execute(
                    "UPDATE entities SET audience = ?2 WHERE id = ?1",
                    params![entity_id, json(audience)?],
                )?;
                update_doc_audience(tx, entity_id, audience)?;
            }
        }
        Ok(())
    }

    // --------------------------------------------------------------- access

    /// The class of a visible identifier. An invisible or absent identifier
    /// is reported the same way: `NotFound`.
    pub fn kind_of(&self, principal: &Principal, id: &str) -> Result<Kind> {
        let (kind, access) = self.raw_kind(id)?.ok_or_else(|| Error::NotFound(id.to_string()))?;
        if !self.visible_access(principal, &access)? {
            return Err(Error::NotFound(id.to_string()));
        }
        Ok(kind)
    }

    pub(crate) fn raw_kind(&self, id: &str) -> Result<Option<(Kind, Access)>> {
        let row: Option<(String, String, String)> = self
            .conn
            .query_row(
                "SELECT class, owner, audience FROM entities WHERE id = ?1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        match row {
            None => Ok(None),
            Some((class, owner, audience)) => {
                let kind = Kind::parse(&class).ok_or_else(|| Error::Integrity(format!("unknown class {class}")))?;
                Ok(Some((
                    kind,
                    Access {
                        owner,
                        audience: serde_json::from_str(&audience)?,
                    },
                )))
            }
        }
    }

    /// Visibility of one identifier. Relationships are visible only when
    /// both ends are; instances and revisions only when their artifact is.
    pub(crate) fn visible(&self, principal: &Principal, id: &str) -> Result<bool> {
        let Some((kind, access)) = self.raw_kind(id)? else {
            return Ok(false);
        };
        if !self.visible_access(principal, &access)? {
            return Ok(false);
        }
        match kind {
            Kind::Relationship => {
                let relationship: Relationship = self.read("relationships", id)?;
                Ok(self.visible(principal, &relationship.source_id)?
                    && self.visible(principal, &relationship.target_id)?)
            }
            Kind::Assertion => {
                let assertion: Assertion = self.read("assertions", id)?;
                self.visible(principal, &assertion.relationship_id)
            }
            Kind::Entity(EntityClass::Revision) => {
                let revision: Revision = self.read("revisions", id)?;
                self.visible(principal, &revision.artifact_id)
            }
            Kind::Entity(EntityClass::Instance) => {
                let instance: Instance = self.read("instances", id)?;
                self.visible(principal, &instance.artifact_id)
            }
            Kind::Entity(EntityClass::Presentation) => {
                let presentation: Presentation = self.read("presentations", id)?;
                self.visible(principal, &presentation.member_artifact_id)
            }
            _ => Ok(true),
        }
    }

    pub(crate) fn visible_access(&self, principal: &Principal, access: &Access) -> Result<bool> {
        Ok(principal.can_see(&access.owner, &access.audience))
    }

    pub(crate) fn require_visible(&self, principal: &Principal, id: &str) -> Result<()> {
        if self.visible(principal, id)? {
            Ok(())
        } else {
            Err(Error::NotFound(id.to_string()))
        }
    }

    /// Every id a change references must be visible for the change to be.
    pub(crate) fn change_visible(&self, principal: &Principal, change: &Change) -> Result<bool> {
        if principal.actor == self.owner {
            return Ok(true);
        }
        let (class, subject) = change.subject();
        if !matches!(class, EntityClassOrOther::Policy) && !self.visible(principal, &subject)? {
            return Ok(false);
        }
        for id in change.referenced_ids() {
            if !self.visible(principal, &id)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(crate) fn read<T: DeserializeOwned>(&self, table: &str, id: &str) -> Result<T> {
        let doc: Option<String> = self
            .conn
            .query_row(&format!("SELECT doc FROM {table} WHERE id = ?1"), params![id], |row| row.get(0))
            .optional()?;
        let doc = doc.ok_or_else(|| Error::NotFound(id.to_string()))?;
        Ok(serde_json::from_str(&doc)?)
    }

    fn read_visible<T: DeserializeOwned>(&self, principal: &Principal, table: &str, id: &str) -> Result<T> {
        self.require_visible(principal, id)?;
        self.read(table, id)
    }

    pub fn get_thing(&self, principal: &Principal, id: &str) -> Result<Thing> {
        self.read_visible(principal, "things", id)
    }

    pub fn get_artifact(&self, principal: &Principal, id: &str) -> Result<Artifact> {
        self.read_visible(principal, "artifacts", id)
    }

    pub fn get_revision(&self, principal: &Principal, id: &str) -> Result<Revision> {
        self.read_visible(principal, "revisions", id)
    }

    pub fn get_instance(&self, principal: &Principal, id: &str) -> Result<Instance> {
        self.read_visible(principal, "instances", id)
    }

    pub fn get_presentation(&self, principal: &Principal, id: &str) -> Result<Presentation> {
        self.read_visible(principal, "presentations", id)
    }

    pub fn get_relationship(&self, principal: &Principal, id: &str) -> Result<Relationship> {
        self.read_visible(principal, "relationships", id)
    }

    pub fn get_assertion(&self, principal: &Principal, id: &str) -> Result<Assertion> {
        self.read_visible(principal, "assertions", id)
    }

    pub fn get_resolution(&self, principal: &Principal, id: &str) -> Result<Resolution> {
        let resolution: Resolution = self.read("resolutions", id)?;
        if self.resolution_visible(principal, &resolution)? {
            Ok(resolution)
        } else {
            Err(Error::NotFound(id.to_string()))
        }
    }

    /// Effective disclosure level of an identifier, for export.
    pub(crate) fn access_of(&self, id: &str) -> Result<Access> {
        self.raw_kind(id)?
            .map(|(_, access)| access)
            .ok_or_else(|| Error::NotFound(id.to_string()))
    }

    pub(crate) fn audience_of(&self, id: &str) -> Result<BTreeSet<String>> {
        Ok(self.access_of(id)?.audience)
    }

    /// Most private audience among several sources (D6): the intersection.
    pub(crate) fn inherited_audience(&self, ids: &[&str]) -> Result<BTreeSet<String>> {
        let mut result: Option<BTreeSet<String>> = None;
        for id in ids {
            let audience = self.audience_of(id)?;
            result = Some(match result {
                None => audience,
                Some(current) => current.intersection(&audience).cloned().collect(),
            });
        }
        Ok(result.unwrap_or_default())
    }

    // ------------------------------------------------------------- entities

    pub fn create_thing(&mut self, principal: &Principal, label: &str, audience: BTreeSet<String>) -> Result<Thing> {
        self.require_owner(principal)?;
        let thing = Thing {
            id: Self::new_id(),
            label: label.to_string(),
            access: self.access_for(audience.clone()),
        };
        self.commit(
            principal,
            Change::ThingCreated { thing: thing.clone() },
            ProvenanceKind::Declared,
            None,
            &audience,
        )?;
        Ok(thing)
    }

    pub fn create_artifact(
        &mut self,
        principal: &Principal,
        label: &str,
        audience: BTreeSet<String>,
    ) -> Result<Artifact> {
        self.require_owner(principal)?;
        let artifact = Artifact {
            id: Self::new_id(),
            label: label.to_string(),
            access: self.access_for(audience.clone()),
        };
        self.commit(
            principal,
            Change::ArtifactCreated {
                artifact: artifact.clone(),
            },
            ProvenanceKind::Declared,
            None,
            &audience,
        )?;
        Ok(artifact)
    }

    pub fn create_revision(
        &mut self,
        principal: &Principal,
        artifact_id: &str,
        content_hash: Option<String>,
        size: Option<u64>,
    ) -> Result<Revision> {
        self.require_owner(principal)?;
        self.get_artifact(principal, artifact_id)?;
        if let Some(hash) = &content_hash {
            sc_envelope::require_sha256("content_hash", hash)?;
        }
        let audience = self.audience_of(artifact_id)?;
        let revision = Revision {
            id: Self::new_id(),
            artifact_id: artifact_id.to_string(),
            content_hash,
            size,
            access: self.access_for(audience.clone()),
        };
        self.commit(
            principal,
            Change::RevisionCreated {
                revision: revision.clone(),
            },
            ProvenanceKind::Declared,
            None,
            &audience,
        )?;
        Ok(revision)
    }

    /// Register a persisted occurrence of a revision at a location. A second
    /// registration for the same revision is a distinct instance (a copy).
    pub fn register_instance(
        &mut self,
        principal: &Principal,
        revision_id: &str,
        location: Location,
        trust_class: &str,
    ) -> Result<Instance> {
        self.require_owner(principal)?;
        let revision = self.get_revision(principal, revision_id)?;
        let audience = self.audience_of(revision_id)?;
        let instance = Instance {
            id: Self::new_id(),
            artifact_id: revision.artifact_id.clone(),
            revision_id: revision_id.to_string(),
            location,
            status: InstanceStatus::Present,
            trust_class: trust_class.to_string(),
            provenance: ProvenanceKind::Declared,
            access: self.access_for(audience.clone()),
        };
        self.commit(
            principal,
            Change::InstanceRegistered {
                instance: instance.clone(),
            },
            ProvenanceKind::Declared,
            None,
            &audience,
        )?;
        Ok(instance)
    }

    /// Artifact, revision and instance for one file in one call.
    pub fn register_file(
        &mut self,
        principal: &Principal,
        label: &str,
        location: Location,
        content_hash: Option<String>,
        audience: BTreeSet<String>,
    ) -> Result<(Artifact, Revision, Instance)> {
        let artifact = self.create_artifact(principal, label, audience)?;
        let revision = self.create_revision(principal, &artifact.id, content_hash, location.size)?;
        let instance = self.register_instance(principal, &revision.id, location, "local")?;
        Ok((artifact, revision, instance))
    }

    pub fn set_audience(&mut self, principal: &Principal, id: &str, audience: BTreeSet<String>) -> Result<()> {
        self.require_owner(principal)?;
        self.require_visible(principal, id)?;
        self.commit(
            principal,
            Change::AudienceChanged {
                entity_id: id.to_string(),
                audience: audience.clone(),
            },
            ProvenanceKind::Declared,
            None,
            &audience,
        )?;
        Ok(())
    }

    /// Revisions of an artifact, newest last.
    pub fn revisions_of(&self, principal: &Principal, artifact_id: &str) -> Result<Vec<Revision>> {
        self.require_visible(principal, artifact_id)?;
        let rows: Vec<Revision> =
            self.query_docs("SELECT doc FROM revisions WHERE artifact_id = ?1 ORDER BY rowid", params![artifact_id])?;
        self.retain_visible(principal, rows, |r| &r.id)
    }

    /// Keep the rows the principal may read one by one, the same rule a
    /// direct `get_*` applies.
    pub(crate) fn retain_visible<T>(&self, principal: &Principal, rows: Vec<T>, id: impl Fn(&T) -> &str) -> Result<Vec<T>> {
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            if self.visible(principal, id(&row))? {
                out.push(row);
            }
        }
        Ok(out)
    }

    /// Instances of an artifact, including missing and unavailable ones.
    pub fn instances_of(&self, principal: &Principal, artifact_id: &str) -> Result<Vec<Instance>> {
        self.require_visible(principal, artifact_id)?;
        let rows: Vec<Instance> =
            self.query_docs("SELECT doc FROM instances WHERE artifact_id = ?1 ORDER BY rowid", params![artifact_id])?;
        self.retain_visible(principal, rows, |i| &i.id)
    }

    pub(crate) fn query_docs<T: DeserializeOwned>(
        &self,
        sql: &str,
        params: impl rusqlite::Params,
    ) -> Result<Vec<T>> {
        let mut statement = self.conn.prepare(sql)?;
        let rows = statement.query_map(params, |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(serde_json::from_str(&row?)?);
        }
        Ok(out)
    }

    /// Disclosure projection of an identifier's audience.
    pub fn disclosure_level(&self, id: &str) -> Result<AccessLevel> {
        Ok(project_audience(&self.audience_of(id)?))
    }
}

pub(crate) fn json<T: Serialize>(value: &T) -> Result<String> {
    Ok(serde_json::to_string(value)?)
}

fn fresh_salt() -> String {
    // Two ULIDs carry 160 random bits between them; the digest spreads them.
    let mut hasher = Sha256::new();
    hasher.update(ulid::Ulid::new().to_bytes());
    hasher.update(ulid::Ulid::new().to_bytes());
    hasher.update(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos().to_le_bytes())
        .unwrap_or([0; 16]));
    format!("{:x}", hasher.finalize())
}

pub(crate) fn salt_in(conn: &Connection) -> Result<String> {
    Ok(conn.query_row("SELECT salt FROM library_secret WHERE id = 'default'", [], |row| row.get(0))?)
}

pub(crate) fn alias_for(salt: &str, hash: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(salt.as_bytes());
    hasher.update([1]);
    hasher.update(hash.as_bytes());
    format!("origin:{:x}", hasher.finalize())
}

pub(crate) fn last_hash_in(tx: &Transaction<'_>) -> Result<Option<String>> {
    Ok(tx
        .query_row("SELECT hash FROM events ORDER BY seq DESC LIMIT 1", [], |row| row.get(0))
        .optional()?)
}

fn policy_owner(tx: &Transaction<'_>) -> Result<String> {
    Ok(tx.query_row("SELECT owner FROM policy WHERE id = 'default'", [], |row| row.get(0))?)
}

fn insert_entity(tx: &Transaction<'_>, id: &str, kind: Kind, label: &str, access: &Access) -> Result<()> {
    let inserted = tx.execute(
        "INSERT OR IGNORE INTO entities (id, class, label, owner, audience) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, kind.as_str(), label, access.owner, json(&access.audience)?],
    )?;
    if inserted == 0 {
        return Err(Error::Integrity(format!("identifier {id} already exists")));
    }
    Ok(())
}

fn require_row(tx: &Transaction<'_>, table: &str, id: &str) -> Result<()> {
    let found: Option<i64> = tx
        .query_row(&format!("SELECT 1 FROM {table} WHERE id = ?1"), params![id], |row| row.get(0))
        .optional()?;
    if found.is_some() {
        Ok(())
    } else {
        Err(Error::Integrity(format!("{table} has no row {id}")))
    }
}

fn require_entity(tx: &Transaction<'_>, id: &str) -> Result<()> {
    require_row(tx, "entities", id)
}

fn read_doc<T: DeserializeOwned>(tx: &Transaction<'_>, table: &str, id: &str) -> Result<T> {
    let doc: Option<String> = tx
        .query_row(&format!("SELECT doc FROM {table} WHERE id = ?1"), params![id], |row| row.get(0))
        .optional()?;
    let doc = doc.ok_or_else(|| Error::Integrity(format!("{table} has no row {id}")))?;
    Ok(serde_json::from_str(&doc)?)
}

fn write_instance(tx: &Transaction<'_>, instance: &Instance, insert: bool) -> Result<()> {
    let sql = if insert {
        "INSERT INTO instances (id, artifact_id, revision_id, volume_uuid, document_id, path, name, size, modified_at, status, doc)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)"
    } else {
        "UPDATE instances SET artifact_id = ?2, revision_id = ?3, volume_uuid = ?4, document_id = ?5, path = ?6,
         name = ?7, size = ?8, modified_at = ?9, status = ?10, doc = ?11 WHERE id = ?1"
    };
    tx.execute(
        sql,
        params![
            instance.id,
            instance.artifact_id,
            instance.revision_id,
            instance.location.volume_uuid,
            instance.location.document_id,
            instance.location.path,
            instance.location.name,
            instance.location.size.map(|s| s as i64),
            instance.location.modified_at,
            instance.status.as_str(),
            json(instance)?
        ],
    )?;
    Ok(())
}

fn write_presentation(tx: &Transaction<'_>, presentation: &Presentation, insert: bool) -> Result<()> {
    let sql = if insert {
        "INSERT INTO presentations (id, container_revision_id, member_path, member_size, member_hash, volume_entity, item_id, mount_epoch, status, doc)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)"
    } else {
        "UPDATE presentations SET container_revision_id = ?2, member_path = ?3, member_size = ?4, member_hash = ?5,
         volume_entity = ?6, item_id = ?7, mount_epoch = ?8, status = ?9, doc = ?10 WHERE id = ?1"
    };
    tx.execute(
        sql,
        params![
            presentation.id,
            presentation.member.container_revision_id,
            presentation.member.member_path,
            presentation.member.member_size as i64,
            presentation.member.member_hash,
            presentation.locator.volume_entity,
            presentation.locator.item_id as i64,
            presentation.locator.mount_epoch as i64,
            presentation.status.as_str(),
            json(presentation)?
        ],
    )?;
    Ok(())
}

fn write_assertion(tx: &Transaction<'_>, assertion: &Assertion, insert: bool) -> Result<()> {
    let sql = if insert {
        "INSERT INTO assertions (id, relationship_id, polarity, status, doc) VALUES (?1, ?2, ?3, ?4, ?5)"
    } else {
        "UPDATE assertions SET relationship_id = ?2, polarity = ?3, status = ?4, doc = ?5 WHERE id = ?1"
    };
    tx.execute(
        sql,
        params![
            assertion.id,
            assertion.relationship_id,
            assertion.polarity.as_str(),
            assertion.status.as_str(),
            json(assertion)?
        ],
    )?;
    Ok(())
}

/// Decision D9: state from accepted assertions only.
pub(crate) fn rematerialize(tx: &Transaction<'_>, relationship_id: &str) -> Result<RelationshipState> {
    let materializes: i64 = tx.query_row(
        "SELECT materializes FROM relationships WHERE id = ?1",
        params![relationship_id],
        |row| row.get(0),
    )?;
    if materializes == 0 {
        // Unregistered predicates stay UNKNOWN whatever is asserted (D3).
        tx.execute(
            "UPDATE relationships SET state = ?2 WHERE id = ?1",
            params![relationship_id, RelationshipState::Unknown.as_str()],
        )?;
        return Ok(RelationshipState::Unknown);
    }
    let count = |polarity: Polarity| -> Result<usize> {
        Ok(tx.query_row(
            "SELECT COUNT(*) FROM assertions WHERE relationship_id = ?1 AND polarity = ?2 AND status = 'accepted'",
            params![relationship_id, polarity.as_str()],
            |row| row.get::<_, i64>(0),
        )? as usize)
    };
    let state = RelationshipState::from_counts(count(Polarity::Supports)?, count(Polarity::Disputes)?);
    tx.execute(
        "UPDATE relationships SET state = ?2 WHERE id = ?1",
        params![relationship_id, state.as_str()],
    )?;
    Ok(state)
}

fn update_doc_audience(tx: &Transaction<'_>, id: &str, audience: &BTreeSet<String>) -> Result<()> {
    let class: String = tx.query_row("SELECT class FROM entities WHERE id = ?1", params![id], |row| row.get(0))?;
    let table = match class.as_str() {
        "thing" => "things",
        "artifact" => "artifacts",
        "revision" => "revisions",
        "instance" => "instances",
        "presentation" => "presentations",
        "relationship" => "relationships",
        "assertion" => "assertions",
        _ => return Ok(()),
    };
    let doc: String = tx.query_row(&format!("SELECT doc FROM {table} WHERE id = ?1"), params![id], |row| row.get(0))?;
    let mut value: serde_json::Value = serde_json::from_str(&doc)?;
    value["access"]["audience"] = serde_json::to_value(audience)?;
    tx.execute(
        &format!("UPDATE {table} SET doc = ?2 WHERE id = ?1"),
        params![id, serde_json::to_string(&value)?],
    )?;
    Ok(())
}
