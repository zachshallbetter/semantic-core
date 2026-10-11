//! The event body carried by `sc-event-0.1` envelopes in the local log.
//!
//! Every event carries the canonical hash of its predecessor inside the body,
//! so the hash of each envelope commits to the whole chain before it
//! (decision D4). The log stores unsigned envelopes; signing happens at the
//! boundary (`interchange`).

use crate::model::*;
use sc_envelope::{Body, ValidationError};
use sc_vocab::EntityClass;
use serde::{Deserialize, Serialize};

/// `compatibility.subject_schema` for every event body.
pub const EVENT_SCHEMA: &str = "sc-change-1";

/// One logical change to graph state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", content = "data", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Change {
    PolicyEstablished {
        authority: String,
        owner: String,
    },
    ThingCreated {
        thing: Thing,
    },
    ArtifactCreated {
        artifact: Artifact,
    },
    RevisionCreated {
        revision: Revision,
    },
    InstanceRegistered {
        instance: Instance,
    },
    /// A location or status change on an existing instance, driven by a
    /// resolution or an explicit volume report.
    InstanceUpdated {
        instance_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        location: Option<Location>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        revision_id: Option<String>,
        status: InstanceStatus,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resolution_id: Option<String>,
    },
    ObservationRecorded {
        observation: Observation,
        resolution: Resolution,
    },
    PresentationMounted {
        presentation: Presentation,
    },
    PresentationRebound {
        presentation_id: String,
        locator: MountLocator,
        valid_from: String,
    },
    PresentationUnavailable {
        presentation_id: String,
        valid_until: String,
    },
    RelationshipDeclared {
        relationship: Relationship,
    },
    AssertionProposed {
        assertion: Assertion,
    },
    QualificationRecorded {
        assertion_id: String,
        qualification: Qualification,
    },
    AssertionAccepted {
        assertion_id: String,
        authority: String,
    },
    /// Compensating event. The assertion's evidence stays on record.
    AssertionRetracted {
        assertion_id: String,
        reason: String,
        /// Canonical hash of the event being compensated.
        compensates: String,
    },
    AudienceChanged {
        entity_id: String,
        audience: std::collections::BTreeSet<String>,
    },
}

impl Change {
    /// The entity the event is about; used as the envelope subject.
    pub fn subject(&self) -> (EntityClassOrOther, String) {
        use EntityClassOrOther as C;
        match self {
            Change::PolicyEstablished { .. } => (C::Policy, "default".into()),
            Change::ThingCreated { thing } => (C::Entity(EntityClass::Thing), thing.id.clone()),
            Change::ArtifactCreated { artifact } => (C::Entity(EntityClass::Artifact), artifact.id.clone()),
            Change::RevisionCreated { revision } => (C::Entity(EntityClass::Revision), revision.id.clone()),
            Change::InstanceRegistered { instance } => (C::Entity(EntityClass::Instance), instance.id.clone()),
            Change::InstanceUpdated { instance_id, .. } => (C::Entity(EntityClass::Instance), instance_id.clone()),
            Change::ObservationRecorded { observation, .. } => (C::Observation, observation.id.clone()),
            Change::PresentationMounted { presentation } => {
                (C::Entity(EntityClass::Presentation), presentation.id.clone())
            }
            Change::PresentationRebound { presentation_id, .. }
            | Change::PresentationUnavailable { presentation_id, .. } => {
                (C::Entity(EntityClass::Presentation), presentation_id.clone())
            }
            Change::RelationshipDeclared { relationship } => (C::Relationship, relationship.id.clone()),
            Change::AssertionProposed { assertion } => (C::Assertion, assertion.id.clone()),
            Change::QualificationRecorded { assertion_id, .. }
            | Change::AssertionAccepted { assertion_id, .. }
            | Change::AssertionRetracted { assertion_id, .. } => (C::Assertion, assertion_id.clone()),
            Change::AudienceChanged { entity_id, .. } => (C::Other, entity_id.clone()),
        }
    }

    /// Every identifier this change references. Import uses it for
    /// referential integrity; export uses it for access.
    pub fn referenced_ids(&self) -> Vec<String> {
        match self {
            Change::PolicyEstablished { .. } => vec![],
            Change::ThingCreated { .. } | Change::ArtifactCreated { .. } => vec![],
            Change::RevisionCreated { revision } => vec![revision.artifact_id.clone()],
            Change::InstanceRegistered { instance } => {
                vec![instance.artifact_id.clone(), instance.revision_id.clone()]
            }
            Change::InstanceUpdated {
                instance_id,
                revision_id,
                resolution_id,
                ..
            } => {
                let mut ids = vec![instance_id.clone()];
                ids.extend(revision_id.clone());
                ids.extend(resolution_id.clone());
                ids
            }
            Change::ObservationRecorded { observation, resolution } => {
                let mut ids = Vec::new();
                ids.extend(observation.instance_id.clone());
                ids.extend(resolution.instance_id.clone());
                ids.extend(resolution.candidates.iter().cloned());
                ids
            }
            Change::PresentationMounted { presentation } => vec![
                presentation.member.container_revision_id.clone(),
                presentation.member_artifact_id.clone(),
                presentation.member_revision_id.clone(),
            ],
            Change::PresentationRebound { presentation_id, .. }
            | Change::PresentationUnavailable { presentation_id, .. } => vec![presentation_id.clone()],
            Change::RelationshipDeclared { relationship } => {
                vec![relationship.source_id.clone(), relationship.target_id.clone()]
            }
            Change::AssertionProposed { assertion } => vec![assertion.relationship_id.clone()],
            Change::QualificationRecorded { assertion_id, .. }
            | Change::AssertionAccepted { assertion_id, .. }
            | Change::AssertionRetracted { assertion_id, .. } => vec![assertion_id.clone()],
            Change::AudienceChanged { entity_id, .. } => vec![entity_id.clone()],
        }
    }

    /// Identifiers this change brings into existence.
    pub fn created_ids(&self) -> Vec<String> {
        match self {
            Change::ThingCreated { thing } => vec![thing.id.clone()],
            Change::ArtifactCreated { artifact } => vec![artifact.id.clone()],
            Change::RevisionCreated { revision } => vec![revision.id.clone()],
            Change::InstanceRegistered { instance } => vec![instance.id.clone()],
            Change::ObservationRecorded { observation, resolution } => {
                vec![observation.id.clone(), resolution.id.clone()]
            }
            Change::PresentationMounted { presentation } => vec![presentation.id.clone()],
            Change::RelationshipDeclared { relationship } => vec![relationship.id.clone()],
            Change::AssertionProposed { assertion } => vec![assertion.id.clone()],
            _ => vec![],
        }
    }
}

impl Change {
    /// Every `Access` block carried by the change.
    pub fn access_blocks_mut(&mut self) -> Vec<&mut Access> {
        match self {
            Change::ThingCreated { thing } => vec![&mut thing.access],
            Change::ArtifactCreated { artifact } => vec![&mut artifact.access],
            Change::RevisionCreated { revision } => vec![&mut revision.access],
            Change::InstanceRegistered { instance } => vec![&mut instance.access],
            Change::PresentationMounted { presentation } => vec![&mut presentation.access],
            Change::RelationshipDeclared { relationship } => vec![&mut relationship.access],
            Change::AssertionProposed { assertion } => vec![&mut assertion.access],
            _ => vec![],
        }
    }

    /// Owners named by the change's access blocks.
    pub fn owners(&self) -> std::collections::BTreeSet<String> {
        let mut copy = self.clone();
        copy.access_blocks_mut().into_iter().map(|a| a.owner.clone()).collect()
    }

    /// Re-base every access block onto `owner`. Returns the owners replaced.
    pub fn rewrite_owner(&mut self, owner: &str) -> std::collections::BTreeSet<String> {
        let mut replaced = std::collections::BTreeSet::new();
        for access in self.access_blocks_mut() {
            if access.owner != owner {
                replaced.insert(std::mem::replace(&mut access.owner, owner.to_string()));
            }
        }
        replaced
    }

    /// Replace raw audience labels with the disclosure projection that
    /// leaves the device (D6).
    pub fn project_access(&mut self) {
        for access in self.access_blocks_mut() {
            access.audience = projected_audience(&access.audience);
        }
        if let Change::AudienceChanged { audience, .. } = self {
            *audience = projected_audience(audience);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityClassOrOther {
    Entity(EntityClass),
    Relationship,
    Assertion,
    Observation,
    Policy,
    Other,
}

impl EntityClassOrOther {
    pub fn prefix(self) -> &'static str {
        match self {
            Self::Entity(class) => class.as_str(),
            Self::Relationship => "relationship",
            Self::Assertion => "assertion",
            Self::Observation => "observation",
            Self::Policy => "policy",
            Self::Other => "entity",
        }
    }
}

/// The body of every event envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventRecord {
    /// Canonical hash of the preceding event in this log; `None` only for the
    /// first event.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous: Option<String>,
    pub actor: String,
    pub authority: String,
    pub at: String,
    /// Canonical hash of the event this one was imported from, when it
    /// entered through `import`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
    /// When the event entered this log through `import`; `at` keeps the
    /// source's time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imported_at: Option<String>,
    /// The owner named by the source when import re-based the change onto
    /// the local owner.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_owner: Option<String>,
    pub change: Change,
}

impl Body for EventRecord {
    fn validate(&self) -> Result<(), ValidationError> {
        sc_envelope::require_nonempty("body.actor", &self.actor)?;
        sc_envelope::require_nonempty("body.authority", &self.authority)?;
        sc_envelope::parse_timestamp("body.at", Some(&self.at))?;
        if let Some(previous) = &self.previous {
            require_event_ref("body.previous", previous)?;
        }
        if let Some(origin) = &self.origin {
            require_event_ref("body.origin", origin)?;
        }
        if let Some(imported_at) = &self.imported_at {
            sc_envelope::parse_timestamp("body.imported_at", Some(imported_at))?;
        }
        Ok(())
    }
}

/// An event reference: a stored hash (`sha256:`), a per-library export alias
/// (`origin:`) or a per-viewer identifier (`view:`), each over 64 hex digits.
pub fn require_event_ref(field: &'static str, value: &str) -> Result<(), ValidationError> {
    for prefix in ["sha256:", "origin:", "view:"] {
        if let Some(digest) = value.strip_prefix(prefix) {
            return sc_envelope::require_sha256(field, &format!("sha256:{digest}")).map_err(|_| {
                ValidationError::InvalidValue {
                    field,
                    value: value.to_string(),
                }
            });
        }
    }
    Err(ValidationError::InvalidValue {
        field,
        value: value.to_string(),
    })
}

pub type EventEnvelope = sc_envelope::Envelope<EventRecord>;

/// A stored event: its position, identity and envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub seq: i64,
    /// Canonical hash of the envelope.
    pub hash: String,
    pub envelope: EventEnvelope,
}
