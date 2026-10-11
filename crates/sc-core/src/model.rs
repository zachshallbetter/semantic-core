//! Entity records, states and the principal used for access checks.

use sc_envelope::AccessLevel;
use sc_vocab::EntityClass;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt;

/// The audience label that means "anyone".
pub const PUBLIC_AUDIENCE: &str = "public";

/// Who is asking. Access checks compare the principal with each object's
/// owner and audience set (decision D6). The owner sees everything it owns;
/// anyone else sees an object only when one of their audiences is in the
/// object's audience set, or the object is public.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Principal {
    pub actor: String,
    pub audiences: BTreeSet<String>,
}

impl Principal {
    pub fn new(actor: impl Into<String>, audiences: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            actor: actor.into(),
            audiences: audiences.into_iter().map(Into::into).collect(),
        }
    }

    pub fn solo(actor: impl Into<String>) -> Self {
        Self::new(actor, Vec::<String>::new())
    }

    pub fn can_see(&self, owner: &str, audience: &BTreeSet<String>) -> bool {
        self.actor == owner
            || audience.contains(PUBLIC_AUDIENCE)
            || audience.iter().any(|label| self.audiences.contains(label))
    }
}

/// Disclosure projection applied when an object leaves the device (D6):
/// owner-only maps to `owner`, any household audience to `member`, public to
/// `public`.
pub fn project_audience(audience: &BTreeSet<String>) -> AccessLevel {
    if audience.contains(PUBLIC_AUDIENCE) {
        AccessLevel::Public
    } else if audience.is_empty() {
        AccessLevel::Owner
    } else {
        AccessLevel::Member
    }
}

/// Shared ownership and audience fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Access {
    pub owner: String,
    #[serde(default)]
    pub audience: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Thing {
    pub id: String,
    pub label: String,
    pub access: Access,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub id: String,
    pub label: String,
    pub access: Access,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Revision {
    pub id: String,
    pub artifact_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    pub access: Access,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InstanceStatus {
    Present,
    Missing,
    Unavailable,
}

impl InstanceStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Present => "present",
            Self::Missing => "missing",
            Self::Unavailable => "unavailable",
        }
    }
}

/// A filesystem location. The within-volume key is `(volume_uuid,
/// document_id)` (decision D2); the path and the rest are observations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Location {
    pub volume_uuid: String,
    pub document_id: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Instance {
    pub id: String,
    pub artifact_id: String,
    pub revision_id: String,
    pub location: Location,
    pub status: InstanceStatus,
    /// How the system may treat this instance; never inferred from provenance
    /// or a signature.
    pub trust_class: String,
    pub provenance: sc_envelope::ProvenanceKind,
    pub access: Access,
}

/// Durable identity of a container member (decision D7).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemberKey {
    pub container_revision_id: String,
    pub member_path: String,
    pub member_size: u64,
    pub member_hash: String,
}

/// Session-scoped locator of a mounted member (decision D7).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MountLocator {
    pub volume_entity: String,
    pub item_id: u64,
    pub mount_epoch: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PresentationStatus {
    Available,
    Unavailable,
}

impl PresentationStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Presentation {
    pub id: String,
    pub member: MemberKey,
    /// The member's durable artifact and revision inside the registry.
    pub member_artifact_id: String,
    pub member_revision_id: String,
    pub locator: MountLocator,
    pub status: PresentationStatus,
    pub valid_from: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_until: Option<String>,
    pub access: Access,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RelationshipState {
    Present,
    Disputed,
    Absent,
    Unknown,
}

impl RelationshipState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Present => "PRESENT",
            Self::Disputed => "DISPUTED",
            Self::Absent => "ABSENT",
            Self::Unknown => "UNKNOWN",
        }
    }

    /// Decision D9, from accepted assertions only.
    pub fn from_counts(supports: usize, disputes: usize) -> Self {
        match (supports > 0, disputes > 0) {
            (true, false) => Self::Present,
            (true, true) => Self::Disputed,
            (false, true) => Self::Absent,
            (false, false) => Self::Unknown,
        }
    }
}

impl fmt::Display for RelationshipState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Relationship {
    pub id: String,
    pub predicate: String,
    pub vocab_version: String,
    pub source_id: String,
    pub source_class: EntityClass,
    pub target_id: String,
    pub target_class: EntityClass,
    /// False for unregistered predicates preserved as values (D3).
    pub materializes: bool,
    pub access: Access,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Polarity {
    Supports,
    Disputes,
}

impl Polarity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Supports => "SUPPORTS",
            Self::Disputes => "DISPUTES",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "SUPPORTS" => Some(Self::Supports),
            "DISPUTES" => Some(Self::Disputes),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AssertionStatus {
    Proposed,
    Accepted,
    Retracted,
}

impl AssertionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Proposed => "proposed",
            Self::Accepted => "accepted",
            Self::Retracted => "retracted",
        }
    }
}

/// EQP-shaped qualification result. Recording one never changes an
/// assertion's status (section 4: qualification is not promotion).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Qualification {
    Pass,
    Blocked,
    Inconclusive,
    NotRun,
}

impl Qualification {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Blocked => "BLOCKED",
            Self::Inconclusive => "INCONCLUSIVE",
            Self::NotRun => "NOT_RUN",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    pub kind: String,
    pub value: String,
}

impl Evidence {
    pub fn new(kind: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            value: value.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assertion {
    pub id: String,
    pub relationship_id: String,
    pub polarity: Polarity,
    pub actor: String,
    #[serde(default)]
    pub evidence: Vec<Evidence>,
    pub status: AssertionStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accepted_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qualification: Option<Qualification>,
    pub access: Access,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ObservationKind {
    /// A file seen at a location.
    Seen,
    /// A known instance's location no longer holds it.
    Gone,
    /// A whole volume is not reachable.
    VolumeUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub id: String,
    pub kind: ObservationKind,
    pub source: String,
    pub observed_at: String,
    pub expires_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<Location>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume_uuid: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Outcome {
    Resolved,
    Ambiguous,
    Unresolved,
}

impl Outcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Resolved => "RESOLVED",
            Self::Ambiguous => "AMBIGUOUS",
            Self::Unresolved => "UNRESOLVED",
        }
    }
}

impl fmt::Display for Outcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Which decision D2 rule fired. The inspector prints these names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Rule {
    /// `(volume UUID, document identifier)` matched a known instance.
    WithinVolumeKey,
    /// Exactly one candidate matched the content hash and the previously known
    /// instance of that revision was gone.
    ContentHashSingleCandidatePreviousGone,
    /// The content hash matched more than one candidate.
    ContentHashMultipleCandidates,
    /// The content hash matched one candidate that still exists: a copy.
    ContentHashCandidateStillPresent,
    /// Only name, size and modification time matched.
    NameSizeMtimeOnly,
    /// Nothing matched.
    NoMatch,
    /// A known location was reported empty.
    KnownLocationGone,
    /// The instance's volume was reported unreachable.
    VolumeUnreachable,
}

impl Rule {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WithinVolumeKey => "within-volume-key",
            Self::ContentHashSingleCandidatePreviousGone => "content-hash-single-candidate-previous-gone",
            Self::ContentHashMultipleCandidates => "content-hash-multiple-candidates",
            Self::ContentHashCandidateStillPresent => "content-hash-candidate-still-present",
            Self::NameSizeMtimeOnly => "name-size-mtime-only",
            Self::NoMatch => "no-match",
            Self::KnownLocationGone => "known-location-gone",
            Self::VolumeUnreachable => "volume-unreachable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resolution {
    pub id: String,
    pub observation_id: String,
    pub outcome: Outcome,
    pub rule: Rule,
    #[serde(default)]
    pub evidence: Vec<Evidence>,
    /// The instance the observation was bound to, when `RESOLVED`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance_id: Option<String>,
    /// Candidate instance ids considered, for `AMBIGUOUS`.
    #[serde(default)]
    pub candidates: Vec<String>,
    /// Whether the content changed under a within-volume match (an edit).
    #[serde(default)]
    pub content_changed: bool,
}
