//! Container members and presentations (section 10, decision D7).
//!
//! A mounted member is a presentation with a session-scoped locator and a
//! validity window. Its durable identity is the member key `(container
//! revision, member path, size, checksum)`, which maps to a member artifact
//! and revision in the registry. Unmounting makes the presentation
//! `unavailable`; remounting re-binds the same presentation through the member
//! key, never through the item id. Extraction creates a new instance with
//! `derived` provenance and a `DERIVED_FROM` relationship to the member.

use crate::error::{Error, Result};
use crate::events::Change;
use crate::model::*;
use crate::store::Store;
use rusqlite::params;
use sc_envelope::ProvenanceKind;

/// A member as a container enumerator reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberSpec {
    pub path: String,
    pub size: u64,
    pub hash: String,
}

impl Store {
    fn presentation_for_member(&self, key: &MemberKey) -> Result<Option<Presentation>> {
        let mut found: Vec<Presentation> = self.query_docs(
            "SELECT doc FROM presentations WHERE container_revision_id = ?1 AND member_path = ?2
             AND member_size = ?3 AND member_hash = ?4 ORDER BY rowid",
            params![key.container_revision_id, key.member_path, key.member_size as i64, key.member_hash],
        )?;
        Ok(found.pop())
    }

    /// Record that a member of a container is browsable at a mount. Returns
    /// the presentation, re-bound when the member was seen before.
    pub fn mount_member(
        &mut self,
        principal: &Principal,
        container_revision_id: &str,
        member: MemberSpec,
        locator: MountLocator,
    ) -> Result<Presentation> {
        self.require_owner(principal)?;
        sc_envelope::require_sha256("member.hash", &member.hash)?;
        let container = self.get_revision(principal, container_revision_id)?;
        let key = MemberKey {
            container_revision_id: container_revision_id.into(),
            member_path: member.path.clone(),
            member_size: member.size,
            member_hash: member.hash.clone(),
        };
        if let Some(existing) = self.presentation_for_member(&key)? {
            let valid_from = self.now();
            let audience = self.audience_of(&existing.id)?;
            self.commit(
                principal,
                Change::PresentationRebound {
                    presentation_id: existing.id.clone(),
                    locator,
                    valid_from,
                },
                ProvenanceKind::Observed,
                Some(self.window()),
                &audience,
            )?;
            return self.get_presentation(principal, &existing.id);
        }
        let audience = self.audience_of(&container.artifact_id)?;
        let member_artifact = self.create_artifact(principal, &member.path, audience.clone())?;
        let member_revision =
            self.create_revision(principal, &member_artifact.id, Some(member.hash.clone()), Some(member.size))?;
        let presentation = Presentation {
            id: Self::new_id(),
            member: key,
            member_artifact_id: member_artifact.id,
            member_revision_id: member_revision.id,
            locator,
            status: PresentationStatus::Available,
            valid_from: self.now(),
            valid_until: None,
            access: self.access_for(audience.clone()),
        };
        self.commit(
            principal,
            Change::PresentationMounted {
                presentation: presentation.clone(),
            },
            ProvenanceKind::Observed,
            Some(self.window()),
            &audience,
        )?;
        Ok(presentation)
    }

    fn window(&self) -> (String, String) {
        let now = self.now_time();
        let fmt = |t: time::OffsetDateTime| t.format(&time::format_description::well_known::Rfc3339).expect("rfc3339");
        (fmt(now), fmt(now + time::Duration::hours(1)))
    }

    /// The mount went away: every presentation on it becomes unavailable.
    /// Nothing is created or deleted.
    pub fn unmount(&mut self, principal: &Principal, volume_entity: &str, mount_epoch: u64) -> Result<Vec<Presentation>> {
        self.require_owner(principal)?;
        let presentations: Vec<Presentation> = self.query_docs(
            "SELECT doc FROM presentations WHERE volume_entity = ?1 AND mount_epoch = ?2 AND status = 'available' ORDER BY rowid",
            params![volume_entity, mount_epoch as i64],
        )?;
        let mut out = Vec::new();
        for presentation in presentations {
            let audience = self.audience_of(&presentation.id)?;
            self.commit(
                principal,
                Change::PresentationUnavailable {
                    presentation_id: presentation.id.clone(),
                    valid_until: self.now(),
                },
                ProvenanceKind::Observed,
                Some(self.window()),
                &audience,
            )?;
            out.push(self.get_presentation(principal, &presentation.id)?);
        }
        Ok(out)
    }

    /// Extract a mounted member to a persisted location: a new instance of
    /// the member's revision, `derived`, related to the member by
    /// `DERIVED_FROM`.
    pub fn extract_member(
        &mut self,
        principal: &Principal,
        presentation_id: &str,
        location: Location,
    ) -> Result<(Instance, Relationship, Assertion)> {
        self.require_owner(principal)?;
        let presentation = self.get_presentation(principal, presentation_id)?;
        if presentation.status != PresentationStatus::Available {
            return Err(Error::Conflict(format!("{presentation_id} is not available to read")));
        }
        let audience = self.inherited_audience(&[presentation_id, &presentation.member_artifact_id])?;
        let instance = Instance {
            id: Self::new_id(),
            artifact_id: presentation.member_artifact_id.clone(),
            revision_id: presentation.member_revision_id.clone(),
            location,
            status: InstanceStatus::Present,
            trust_class: "local".into(),
            provenance: ProvenanceKind::Derived,
            access: self.access_for(audience.clone()),
        };
        self.commit(
            principal,
            Change::InstanceRegistered {
                instance: instance.clone(),
            },
            ProvenanceKind::Derived,
            None,
            &audience,
        )?;
        let (relationship, assertion) = self.link(
            principal,
            "DERIVED_FROM",
            &instance.id,
            &presentation.member_revision_id,
            vec![
                Evidence::new("presentation", presentation_id),
                Evidence::new("member-hash", presentation.member.member_hash.clone()),
                Evidence::new("container-revision", presentation.member.container_revision_id.clone()),
            ],
        )?;
        Ok((instance, relationship, assertion))
    }

    pub fn presentations_of_container(&self, principal: &Principal, container_revision_id: &str) -> Result<Vec<Presentation>> {
        self.require_visible(principal, container_revision_id)?;
        self.query_docs(
            "SELECT doc FROM presentations WHERE container_revision_id = ?1 ORDER BY rowid",
            params![container_revision_id],
        )
    }
}
