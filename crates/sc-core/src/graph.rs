//! Relationships, assertions, materialized state, access-checked queries,
//! history and `why`.

use crate::error::{Error, Result};
use crate::events::{Change, Event};
use crate::model::*;
use crate::store::{Kind, Store};
use rusqlite::params;
use sc_envelope::ProvenanceKind;
use sc_vocab::{EntityClass, Predicate, VOCAB_VERSION};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Outgoing,
    Incoming,
}

/// A relationship as seen from one of its ends.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelationshipView {
    pub relationship: Relationship,
    pub state: RelationshipState,
    pub direction: Direction,
    /// The predicate, or its inverse label when read from the target.
    pub label: String,
    /// The other end.
    pub other_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchHit {
    pub id: String,
    pub kind: String,
    pub label: String,
}

/// Everything behind one relationship.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Why {
    pub relationship: Relationship,
    pub state: RelationshipState,
    pub assertions: Vec<Assertion>,
    /// Resolutions of the instance endpoints, so a survived move is visible.
    pub resolutions: Vec<Resolution>,
    pub events: Vec<Event>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Inspection {
    Thing {
        thing: Thing,
        relationships: Vec<RelationshipView>,
    },
    Artifact {
        artifact: Artifact,
        revisions: Vec<Revision>,
        instances: Vec<Instance>,
        relationships: Vec<RelationshipView>,
    },
    Revision {
        revision: Revision,
        relationships: Vec<RelationshipView>,
    },
    Instance {
        instance: Instance,
        resolutions: Vec<Resolution>,
        relationships: Vec<RelationshipView>,
    },
    Presentation {
        presentation: Presentation,
        relationships: Vec<RelationshipView>,
    },
    Relationship {
        relationship: Relationship,
        state: RelationshipState,
        assertions: Vec<Assertion>,
    },
    Assertion {
        assertion: Assertion,
    },
}

impl Store {
    fn entity_class(&self, principal: &Principal, id: &str) -> Result<EntityClass> {
        match self.kind_of(principal, id)? {
            Kind::Entity(class) => Ok(class),
            other => Err(Error::Conflict(format!("{id} is a {}, not an entity", other.as_str()))),
        }
    }

    /// Declare the canonical relationship for a triple. Declaring the same
    /// triple twice returns the existing relationship. The vocabulary's class
    /// constraints are checked here; unregistered predicates are kept as
    /// values and never materialize (D3).
    pub fn declare_relationship(
        &mut self,
        principal: &Principal,
        predicate: &str,
        source_id: &str,
        target_id: &str,
    ) -> Result<Relationship> {
        self.require_owner(principal)?;
        let predicate = Predicate::parse_for_declaration(predicate)?;
        let source_class = self.entity_class(principal, source_id)?;
        let target_class = self.entity_class(principal, target_id)?;
        predicate.check(source_class, target_class)?;
        let value = predicate.as_value();
        if let Some(existing) = self.find_relationship(&value, source_id, target_id)? {
            return Ok(existing);
        }
        let audience = self.inherited_audience(&[source_id, target_id])?;
        let relationship = Relationship {
            id: Self::new_id(),
            predicate: value,
            vocab_version: VOCAB_VERSION.into(),
            source_id: source_id.into(),
            target_id: target_id.into(),
            source_class,
            target_class,
            materializes: predicate.materializes(),
            access: self.access_for(audience.clone()),
        };
        self.commit(
            principal,
            Change::RelationshipDeclared {
                relationship: relationship.clone(),
            },
            ProvenanceKind::Declared,
            None,
            &audience,
        )?;
        Ok(relationship)
    }

    fn find_relationship(&self, predicate: &str, source_id: &str, target_id: &str) -> Result<Option<Relationship>> {
        let mut found: Vec<Relationship> = self.query_docs(
            "SELECT doc FROM relationships WHERE predicate = ?1 AND source_id = ?2 AND target_id = ?3",
            params![predicate, source_id, target_id],
        )?;
        Ok(found.pop())
    }

    /// Propose an assertion about a relationship. It carries its own
    /// evidence and starts `proposed`; nothing here accepts it.
    pub fn propose_assertion(
        &mut self,
        principal: &Principal,
        relationship_id: &str,
        polarity: Polarity,
        evidence: Vec<Evidence>,
    ) -> Result<Assertion> {
        self.require_owner(principal)?;
        let relationship = self.get_relationship(principal, relationship_id)?;
        require_materialized(&relationship)?;
        let audience = self.audience_of(relationship_id)?;
        let assertion = Assertion {
            id: Self::new_id(),
            relationship_id: relationship_id.into(),
            polarity,
            actor: principal.actor.clone(),
            evidence,
            status: AssertionStatus::Proposed,
            accepted_by: None,
            qualification: None,
            access: self.access_for(audience.clone()),
        };
        self.commit(
            principal,
            Change::AssertionProposed {
                assertion: assertion.clone(),
            },
            ProvenanceKind::Declared,
            None,
            &audience,
        )?;
        Ok(assertion)
    }

    /// Record what the evidence justifies. This never changes the
    /// assertion's status: qualification is not promotion.
    pub fn record_qualification(
        &mut self,
        principal: &Principal,
        assertion_id: &str,
        qualification: Qualification,
    ) -> Result<Assertion> {
        self.require_owner(principal)?;
        let assertion = self.get_assertion(principal, assertion_id)?;
        let audience = self.audience_of(&assertion.relationship_id)?;
        self.commit(
            principal,
            Change::QualificationRecorded {
                assertion_id: assertion_id.into(),
                qualification,
            },
            ProvenanceKind::Derived,
            None,
            &audience,
        )?;
        self.get_assertion(principal, assertion_id)
    }

    /// Accept an assertion under a named authority. Only the library's
    /// policy authority is recognised in Stage 1 (D6), and only the owner
    /// may invoke it.
    pub fn accept_assertion(&mut self, principal: &Principal, assertion_id: &str, authority: &str) -> Result<Assertion> {
        self.require_owner(principal)?;
        if authority != self.authority {
            return Err(Error::Unauthorized(format!(
                "authority {authority:?} is not recognised; this library accepts {:?}",
                self.authority
            )));
        }
        let assertion = self.get_assertion(principal, assertion_id)?;
        require_materialized(&self.get_relationship(principal, &assertion.relationship_id)?)?;
        match assertion.status {
            AssertionStatus::Proposed => {}
            AssertionStatus::Accepted => return Err(Error::Conflict(format!("{assertion_id} is already accepted"))),
            AssertionStatus::Retracted => return Err(Error::Conflict(format!("{assertion_id} was retracted"))),
        }
        let audience = self.audience_of(&assertion.relationship_id)?;
        self.commit(
            principal,
            Change::AssertionAccepted {
                assertion_id: assertion_id.into(),
                authority: authority.into(),
            },
            ProvenanceKind::Declared,
            None,
            &audience,
        )?;
        self.get_assertion(principal, assertion_id)
    }

    /// Retract an assertion with a compensating event. The original evidence
    /// and events stay on record.
    pub fn retract_assertion(&mut self, principal: &Principal, assertion_id: &str, reason: &str) -> Result<Event> {
        self.require_owner(principal)?;
        let assertion = self.get_assertion(principal, assertion_id)?;
        if assertion.status == AssertionStatus::Retracted {
            return Err(Error::Conflict(format!("{assertion_id} is already retracted")));
        }
        let history = self.history(principal, assertion_id)?;
        let compensates = history
            .iter()
            .rev()
            .find(|event| {
                matches!(
                    event.envelope.body.change,
                    Change::AssertionAccepted { .. } | Change::AssertionProposed { .. }
                )
            })
            .map(|event| event.hash.clone())
            .ok_or_else(|| Error::Integrity(format!("{assertion_id} has no proposal event")))?;
        let audience = self.audience_of(&assertion.relationship_id)?;
        self.commit(
            principal,
            Change::AssertionRetracted {
                assertion_id: assertion_id.into(),
                reason: reason.into(),
                compensates,
            },
            ProvenanceKind::Declared,
            None,
            &audience,
        )
    }

    /// Declare, propose and accept in one step under the policy authority.
    /// Three events are written; acceptance is still its own event.
    pub fn link(
        &mut self,
        principal: &Principal,
        predicate: &str,
        source_id: &str,
        target_id: &str,
        evidence: Vec<Evidence>,
    ) -> Result<(Relationship, Assertion)> {
        let relationship = self.declare_relationship(principal, predicate, source_id, target_id)?;
        let assertion = self.propose_assertion(principal, &relationship.id, Polarity::Supports, evidence)?;
        let authority = self.authority.clone();
        let assertion = self.accept_assertion(principal, &assertion.id, &authority)?;
        Ok((relationship, assertion))
    }

    pub fn relationship_state(&self, principal: &Principal, relationship_id: &str) -> Result<RelationshipState> {
        self.require_visible(principal, relationship_id)?;
        let state: String = self.conn.query_row(
            "SELECT state FROM relationships WHERE id = ?1",
            params![relationship_id],
            |row| row.get(0),
        )?;
        parse_state(&state)
    }

    pub fn assertions_of(&self, principal: &Principal, relationship_id: &str) -> Result<Vec<Assertion>> {
        self.require_visible(principal, relationship_id)?;
        let rows: Vec<Assertion> = self.query_docs(
            "SELECT doc FROM assertions WHERE relationship_id = ?1 ORDER BY rowid",
            params![relationship_id],
        )?;
        self.retain_visible(principal, rows, |a| &a.id)
    }

    /// Materialized relationships touching an entity that the principal may
    /// see. Unregistered predicates and withheld ends are excluded; nothing
    /// in the result or its length reveals them.
    pub fn relationships_of(&self, principal: &Principal, entity_id: &str) -> Result<Vec<RelationshipView>> {
        self.require_visible(principal, entity_id)?;
        let mut statement = self.conn.prepare(
            "SELECT doc, state FROM relationships WHERE (source_id = ?1 OR target_id = ?1) AND materializes = 1 ORDER BY rowid",
        )?;
        let rows = statement.query_map(params![entity_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut views = Vec::new();
        for row in rows {
            let (doc, state) = row?;
            let relationship: Relationship = serde_json::from_str(&doc)?;
            if !self.visible(principal, &relationship.id)? {
                continue;
            }
            let state = parse_state(&state)?;
            let predicate = Predicate::parse(&relationship.predicate)?;
            if relationship.source_id == entity_id {
                views.push(RelationshipView {
                    label: relationship.predicate.clone(),
                    other_id: relationship.target_id.clone(),
                    direction: Direction::Outgoing,
                    state,
                    relationship: relationship.clone(),
                });
            }
            if relationship.target_id == entity_id && relationship.source_id != entity_id {
                views.push(RelationshipView {
                    label: predicate.inverse_label().unwrap_or_else(|| relationship.predicate.clone()),
                    other_id: relationship.source_id.clone(),
                    direction: Direction::Incoming,
                    state,
                    relationship,
                });
            }
        }
        Ok(views)
    }

    /// Count of relationships the principal may see. Computed from the
    /// filtered set, never from the table.
    pub fn count_relationships(&self, principal: &Principal, entity_id: &str) -> Result<usize> {
        Ok(self.relationships_of(principal, entity_id)?.len())
    }

    /// Entities whose label or path contains `text`, filtered by access.
    pub fn search(&self, principal: &Principal, text: &str) -> Result<Vec<SearchHit>> {
        let pattern = format!("%{}%", text.replace('%', "\\%").replace('_', "\\_"));
        let mut statement = self.conn.prepare(
            "SELECT id, class, label FROM entities
             WHERE class IN ('thing', 'artifact', 'instance', 'presentation') AND label LIKE ?1 ESCAPE '\\'
             ORDER BY rowid",
        )?;
        let rows = statement.query_map(params![pattern], |row| {
            Ok(SearchHit {
                id: row.get(0)?,
                kind: row.get(1)?,
                label: row.get(2)?,
            })
        })?;
        let mut hits = Vec::new();
        for hit in rows {
            let hit = hit?;
            if self.visible(principal, &hit.id)? {
                hits.push(hit);
            }
        }
        Ok(hits)
    }

    /// Entities reachable from `start` over visible materialized edges,
    /// within `max_depth` hops. Restricted edges are never crossed, so what
    /// lies behind them is never reached.
    pub fn reachable(&self, principal: &Principal, start: &str, max_depth: usize) -> Result<BTreeSet<String>> {
        self.require_visible(principal, start)?;
        let mut seen = BTreeSet::from([start.to_string()]);
        let mut queue = VecDeque::from([(start.to_string(), 0usize)]);
        while let Some((id, depth)) = queue.pop_front() {
            if depth == max_depth {
                continue;
            }
            for view in self.relationships_of(principal, &id)? {
                if seen.insert(view.other_id.clone()) {
                    queue.push_back((view.other_id, depth + 1));
                }
            }
        }
        seen.remove(start);
        Ok(seen)
    }

    /// Events about an identifier: those with it as subject and those that
    /// reference it. Non-owners see only events whose every reference is
    /// visible to them.
    pub fn history(&self, principal: &Principal, id: &str) -> Result<Vec<Event>> {
        self.require_visible(principal, id)?;
        let mut out = Vec::new();
        for event in self.all_events()? {
            let change = &event.envelope.body.change;
            let (_, subject) = change.subject();
            let touches = subject == id || change.referenced_ids().iter().any(|r| r == id);
            if touches && self.change_visible(principal, change)? {
                let position = out.len() + 1;
                out.push(self.project_event(principal, event, position)?);
            }
        }
        Ok(out)
    }

    /// Why a relationship exists: assertions with their evidence and status,
    /// resolution evidence for instance endpoints, and the full history.
    pub fn why(&self, principal: &Principal, relationship_id: &str) -> Result<Why> {
        let relationship = self.get_relationship(principal, relationship_id)?;
        let state = self.relationship_state(principal, relationship_id)?;
        let assertions = self.assertions_of(principal, relationship_id)?;
        let mut resolutions = Vec::new();
        for (id, class) in [
            (&relationship.source_id, relationship.source_class),
            (&relationship.target_id, relationship.target_class),
        ] {
            if class == EntityClass::Instance {
                resolutions.extend(self.resolutions_for_instance(principal, id)?);
            }
        }
        // History projects positions per call, so merge on the raw log and
        // project once at the end.
        let raw = |id: &str| -> Result<Vec<Event>> {
            let owner = self.owner_principal();
            let mut out = Vec::new();
            for event in self.history(&owner, id)? {
                if self.change_visible(principal, &event.envelope.body.change)? {
                    out.push(event);
                }
            }
            Ok(out)
        };
        let mut events = raw(relationship_id)?;
        for assertion in &assertions {
            for event in raw(&assertion.id)? {
                if !events.iter().any(|e| e.hash == event.hash) {
                    events.push(event);
                }
            }
        }
        // Merge by log identity, then renumber for a restricted viewer so the
        // merged list carries no gaps either.
        events.sort_by_key(|event| event.seq);
        if principal.actor != self.owner() {
            let mut by_hash: Vec<Event> = Vec::new();
            for event in events {
                if !by_hash.iter().any(|e| e.hash == event.hash) {
                    by_hash.push(event);
                }
            }
            let mut projected = Vec::with_capacity(by_hash.len());
            for (i, e) in by_hash.into_iter().enumerate() {
                projected.push(self.project_event(principal, e, i + 1)?);
            }
            events = projected;
        }
        Ok(Why {
            relationship,
            state,
            assertions,
            resolutions,
            events,
        })
    }

    pub fn inspect(&self, principal: &Principal, id: &str) -> Result<Inspection> {
        Ok(match self.kind_of(principal, id)? {
            Kind::Entity(EntityClass::Thing) => Inspection::Thing {
                thing: self.get_thing(principal, id)?,
                relationships: self.relationships_of(principal, id)?,
            },
            Kind::Entity(EntityClass::Artifact) => Inspection::Artifact {
                artifact: self.get_artifact(principal, id)?,
                revisions: self.revisions_of(principal, id)?,
                instances: self.instances_of(principal, id)?,
                relationships: self.relationships_of(principal, id)?,
            },
            Kind::Entity(EntityClass::Revision) => Inspection::Revision {
                revision: self.get_revision(principal, id)?,
                relationships: self.relationships_of(principal, id)?,
            },
            Kind::Entity(EntityClass::Instance) => Inspection::Instance {
                instance: self.get_instance(principal, id)?,
                resolutions: self.resolutions_for_instance(principal, id)?,
                relationships: self.relationships_of(principal, id)?,
            },
            Kind::Entity(EntityClass::Presentation) => Inspection::Presentation {
                presentation: self.get_presentation(principal, id)?,
                relationships: self.relationships_of(principal, id)?,
            },
            Kind::Relationship => Inspection::Relationship {
                relationship: self.get_relationship(principal, id)?,
                state: self.relationship_state(principal, id)?,
                assertions: self.assertions_of(principal, id)?,
            },
            Kind::Assertion => Inspection::Assertion {
                assertion: self.get_assertion(principal, id)?,
            },
            Kind::Observation => return Err(Error::NotFound(id.to_string())),
        })
    }
}

fn require_materialized(relationship: &Relationship) -> Result<()> {
    if relationship.materializes {
        Ok(())
    } else {
        Err(Error::Conflict(format!(
            "relationship {} uses the unregistered predicate {}; it is preserved as a value and takes no assertions until the predicate is mapped",
            relationship.id, relationship.predicate
        )))
    }
}

fn parse_state(text: &str) -> Result<RelationshipState> {
    match text {
        "PRESENT" => Ok(RelationshipState::Present),
        "DISPUTED" => Ok(RelationshipState::Disputed),
        "ABSENT" => Ok(RelationshipState::Absent),
        "UNKNOWN" => Ok(RelationshipState::Unknown),
        other => Err(Error::Integrity(format!("unknown relationship state {other}"))),
    }
}
