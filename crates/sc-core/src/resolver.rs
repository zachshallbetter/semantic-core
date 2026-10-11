//! Identity resolution (decision D2): rules, not scores. Every resolution is
//! recorded with the rule that fired and the evidence it saw, so `why` can
//! print it.

use crate::error::{Error, Result};
use crate::events::Change;
use crate::model::*;
use crate::store::Store;
use rusqlite::params;
use sc_envelope::ProvenanceKind;
use time::format_description::well_known::Rfc3339;

/// How long an observation stays fresh.
const OBSERVATION_TTL: time::Duration = time::Duration::hours(1);

/// A file seen at a location by some source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seen {
    pub location: Location,
    pub content_hash: Option<String>,
    pub source: String,
}

impl Store {
    fn observation_window(&self) -> (String, String) {
        let now = self.now_time();
        (
            now.format(&Rfc3339).expect("rfc3339"),
            (now + OBSERVATION_TTL).format(&Rfc3339).expect("rfc3339"),
        )
    }

    fn record_observation(
        &mut self,
        principal: &Principal,
        observation: Observation,
        resolution: Resolution,
    ) -> Result<Resolution> {
        let window = (observation.observed_at.clone(), observation.expires_at.clone());
        let audience = match &resolution.instance_id {
            Some(id) => self.audience_of(id)?,
            None => Default::default(),
        };
        self.commit(
            principal,
            Change::ObservationRecorded {
                observation,
                resolution: resolution.clone(),
            },
            ProvenanceKind::Observed,
            Some(window),
            &audience,
        )?;
        Ok(resolution)
    }

    fn update_instance(
        &mut self,
        principal: &Principal,
        instance_id: &str,
        location: Option<Location>,
        revision_id: Option<String>,
        status: InstanceStatus,
        resolution_id: &str,
    ) -> Result<()> {
        let audience = self.audience_of(instance_id)?;
        self.commit(
            principal,
            Change::InstanceUpdated {
                instance_id: instance_id.to_string(),
                location,
                revision_id,
                status,
                resolution_id: Some(resolution_id.to_string()),
            },
            ProvenanceKind::Derived,
            None,
            &audience,
        )?;
        Ok(())
    }

    fn instances_where(&self, sql: &str, params: impl rusqlite::Params) -> Result<Vec<Instance>> {
        self.query_docs(sql, params)
    }

    /// Resolve a sighting against the registry. A `RESOLVED` outcome updates
    /// the matched instance's location (and its revision when the content
    /// changed). `AMBIGUOUS` and `UNRESOLVED` change nothing but the record.
    pub fn observe(&mut self, principal: &Principal, seen: Seen) -> Result<Resolution> {
        self.require_owner(principal)?;
        if let Some(hash) = &seen.content_hash {
            sc_envelope::require_sha256("content_hash", hash)?;
        }
        let (observed_at, expires_at) = self.observation_window();
        let observation = Observation {
            id: Self::new_id(),
            kind: ObservationKind::Seen,
            source: seen.source.clone(),
            observed_at,
            expires_at,
            location: Some(seen.location.clone()),
            content_hash: seen.content_hash.clone(),
            instance_id: None,
            volume_uuid: None,
        };
        let mut evidence = Vec::new();
        let resolution_id = Self::new_id();
        let base = |outcome: Outcome, rule: Rule, evidence: Vec<Evidence>| Resolution {
            id: resolution_id.clone(),
            observation_id: observation.id.clone(),
            outcome,
            rule,
            evidence,
            instance_id: None,
            candidates: vec![],
            content_changed: false,
        };

        // Rule 1: the within-volume key.
        let keyed = self.instances_where(
            "SELECT doc FROM instances WHERE volume_uuid = ?1 AND document_id = ?2 ORDER BY rowid",
            params![seen.location.volume_uuid, seen.location.document_id],
        )?;
        evidence.push(Evidence::new(
            "within-volume-key",
            format!(
                "({}, {}) matched {} instance(s)",
                seen.location.volume_uuid,
                seen.location.document_id,
                keyed.len()
            ),
        ));
        if keyed.len() == 1 {
            let instance = &keyed[0];
            if instance.location.path != seen.location.path {
                evidence.push(Evidence::new(
                    "path-changed",
                    format!("{} -> {}", instance.location.path, seen.location.path),
                ));
            }
            if instance.status != InstanceStatus::Present {
                evidence.push(Evidence::new("status", format!("{} -> present", instance.status.as_str())));
            }
            let revision: Revision = self.read("revisions", &instance.revision_id)?;
            let mut new_revision_id = None;
            let mut content_changed = false;
            if let (Some(seen_hash), Some(known_hash)) = (&seen.content_hash, &revision.content_hash) {
                if seen_hash != known_hash {
                    content_changed = true;
                    evidence.push(Evidence::new("content-changed", format!("{known_hash} -> {seen_hash}")));
                    let revision = self.derived_revision(
                        principal,
                        &instance.artifact_id,
                        seen_hash.clone(),
                        seen.location.size,
                    )?;
                    evidence.push(Evidence::new("new-revision", revision.id.clone()));
                    new_revision_id = Some(revision.id);
                } else {
                    evidence.push(Evidence::new("content-hash", "unchanged"));
                }
            }
            let mut resolution = base(Outcome::Resolved, Rule::WithinVolumeKey, evidence);
            resolution.instance_id = Some(instance.id.clone());
            resolution.content_changed = content_changed;
            let resolution = self.record_observation(principal, observation, resolution)?;
            self.update_instance(
                principal,
                &instance.id,
                Some(seen.location.clone()),
                new_revision_id,
                InstanceStatus::Present,
                &resolution.id,
            )?;
            return Ok(resolution);
        }
        if keyed.len() > 1 {
            let mut resolution = base(Outcome::Ambiguous, Rule::ContentHashMultipleCandidates, evidence);
            resolution.candidates = keyed.iter().map(|i| i.id.clone()).collect();
            return self.record_observation(principal, observation, resolution);
        }

        // Rule 2: content hash, across volumes.
        if let Some(hash) = &seen.content_hash {
            let candidates = self.instances_where(
                "SELECT instances.doc FROM instances JOIN revisions ON revisions.id = instances.revision_id
                 WHERE revisions.content_hash = ?1 ORDER BY instances.rowid",
                params![hash],
            )?;
            let artifacts: std::collections::BTreeSet<&str> =
                candidates.iter().map(|c| c.artifact_id.as_str()).collect();
            evidence.push(Evidence::new(
                "content-hash",
                format!(
                    "{hash} matched {} instance(s) across {} artifact(s)",
                    candidates.len(),
                    artifacts.len()
                ),
            ));
            match candidates.as_slice() {
                [] => {}
                [candidate] => {
                    let ids = vec![candidate.id.clone()];
                    if candidate.status == InstanceStatus::Missing {
                        evidence.push(Evidence::new(
                            "previous-instance",
                            format!("{} at {} is missing", candidate.id, candidate.location.path),
                        ));
                        let mut resolution =
                            base(Outcome::Resolved, Rule::ContentHashSingleCandidatePreviousGone, evidence);
                        resolution.instance_id = Some(candidate.id.clone());
                        resolution.candidates = ids;
                        let resolution = self.record_observation(principal, observation, resolution)?;
                        self.update_instance(
                            principal,
                            &candidate.id,
                            Some(seen.location.clone()),
                            None,
                            InstanceStatus::Present,
                            &resolution.id,
                        )?;
                        return Ok(resolution);
                    }
                    evidence.push(Evidence::new(
                        "previous-instance",
                        format!(
                            "{} at {} is {}: this is a copy, not a move",
                            candidate.id,
                            candidate.location.path,
                            candidate.status.as_str()
                        ),
                    ));
                    let mut resolution =
                        base(Outcome::Ambiguous, Rule::ContentHashCandidateStillPresent, evidence);
                    resolution.candidates = ids;
                    return self.record_observation(principal, observation, resolution);
                }
                many => {
                    if artifacts.len() > 1 {
                        evidence.push(Evidence::new(
                            "no-merge",
                            "identical content in more than one artifact; a hash never merges artifacts",
                        ));
                    }
                    let mut resolution = base(Outcome::Ambiguous, Rule::ContentHashMultipleCandidates, evidence);
                    resolution.candidates = many.iter().map(|c| c.id.clone()).collect();
                    return self.record_observation(principal, observation, resolution);
                }
            }
        }

        // Rule 3: name, size and modification time only.
        if let (Some(name), Some(size), Some(modified_at)) =
            (&seen.location.name, seen.location.size, &seen.location.modified_at)
        {
            let weak = self.instances_where(
                "SELECT doc FROM instances WHERE name = ?1 AND size = ?2 AND modified_at = ?3 ORDER BY rowid",
                params![name, size as i64, modified_at],
            )?;
            if !weak.is_empty() {
                evidence.push(Evidence::new(
                    "name-size-mtime",
                    format!("{} instance(s) share name, size and modification time", weak.len()),
                ));
                let mut resolution = base(Outcome::Ambiguous, Rule::NameSizeMtimeOnly, evidence);
                resolution.candidates = weak.iter().map(|i| i.id.clone()).collect();
                return self.record_observation(principal, observation, resolution);
            }
        }

        evidence.push(Evidence::new("no-match", "no instance matched by key, hash or name"));
        let resolution = base(Outcome::Unresolved, Rule::NoMatch, evidence);
        self.record_observation(principal, observation, resolution)
    }

    fn derived_revision(
        &mut self,
        principal: &Principal,
        artifact_id: &str,
        content_hash: String,
        size: Option<u64>,
    ) -> Result<Revision> {
        let audience = self.audience_of(artifact_id)?;
        let revision = Revision {
            id: Self::new_id(),
            artifact_id: artifact_id.to_string(),
            content_hash: Some(content_hash),
            size,
            access: self.access_for(audience.clone()),
        };
        self.commit(
            principal,
            Change::RevisionCreated {
                revision: revision.clone(),
            },
            ProvenanceKind::Derived,
            None,
            &audience,
        )?;
        Ok(revision)
    }

    /// A known instance's location no longer holds it. The instance is
    /// marked `missing`; its revision, artifact and thing are untouched.
    pub fn observe_gone(&mut self, principal: &Principal, instance_id: &str, source: &str) -> Result<Resolution> {
        self.require_owner(principal)?;
        let instance = self.get_instance(principal, instance_id)?;
        let (observed_at, expires_at) = self.observation_window();
        let observation = Observation {
            id: Self::new_id(),
            kind: ObservationKind::Gone,
            source: source.to_string(),
            observed_at,
            expires_at,
            location: Some(instance.location.clone()),
            content_hash: None,
            instance_id: Some(instance_id.to_string()),
            volume_uuid: None,
        };
        let resolution = Resolution {
            id: Self::new_id(),
            observation_id: observation.id.clone(),
            outcome: Outcome::Unresolved,
            rule: Rule::KnownLocationGone,
            evidence: vec![
                Evidence::new("known-location", instance.location.path.clone()),
                Evidence::new("effect", "instance marked missing; artifact and thing untouched"),
            ],
            instance_id: Some(instance_id.to_string()),
            candidates: vec![],
            content_changed: false,
        };
        let resolution = self.record_observation(principal, observation, resolution)?;
        self.update_instance(principal, instance_id, None, None, InstanceStatus::Missing, &resolution.id)?;
        Ok(resolution)
    }

    /// A whole volume is unreachable. Its instances become `unavailable`,
    /// which is distinct from `missing`: nothing is known to be gone.
    pub fn observe_volume_unavailable(
        &mut self,
        principal: &Principal,
        volume_uuid: &str,
        source: &str,
    ) -> Result<Vec<Resolution>> {
        self.require_owner(principal)?;
        let instances = self.instances_where(
            "SELECT doc FROM instances WHERE volume_uuid = ?1 AND status != 'unavailable' ORDER BY rowid",
            params![volume_uuid],
        )?;
        let mut resolutions = Vec::new();
        for instance in instances {
            let (observed_at, expires_at) = self.observation_window();
            let observation = Observation {
                id: Self::new_id(),
                kind: ObservationKind::VolumeUnavailable,
                source: source.to_string(),
                observed_at,
                expires_at,
                location: None,
                content_hash: None,
                instance_id: Some(instance.id.clone()),
                volume_uuid: Some(volume_uuid.to_string()),
            };
            let resolution = Resolution {
                id: Self::new_id(),
                observation_id: observation.id.clone(),
                outcome: Outcome::Unresolved,
                rule: Rule::VolumeUnreachable,
                evidence: vec![Evidence::new("volume", volume_uuid.to_string())],
                instance_id: Some(instance.id.clone()),
                candidates: vec![],
                content_changed: false,
            };
            let resolution = self.record_observation(principal, observation, resolution)?;
            self.update_instance(principal, &instance.id, None, None, InstanceStatus::Unavailable, &resolution.id)?;
            resolutions.push(resolution);
        }
        Ok(resolutions)
    }

    /// Every resolution that bound or concerned an instance, oldest first.
    pub fn resolutions_for_instance(&self, principal: &Principal, instance_id: &str) -> Result<Vec<Resolution>> {
        self.require_visible(principal, instance_id)?;
        self.query_docs(
            "SELECT doc FROM resolutions WHERE instance_id = ?1 ORDER BY rowid",
            params![instance_id],
        )
    }

    pub fn get_observation(&self, principal: &Principal, id: &str) -> Result<Observation> {
        let observation: Observation = self.read("observations", id)?;
        match &observation.instance_id {
            Some(instance_id) => self.require_visible(principal, instance_id)?,
            None => self.require_owner(principal).map_err(|_| Error::NotFound(id.to_string()))?,
        }
        Ok(observation)
    }
}
