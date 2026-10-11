//! Shared helpers: a scenario interpreter for the JSON fixtures under
//! `fixtures/` and a few constructors.

#![allow(dead_code)]

use sc_core::*;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub const OWNER: &str = "ava";

pub fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

pub fn fixed_store() -> Store {
    let mut store = Store::in_memory(OWNER).unwrap();
    store.set_clock(|| {
        time::OffsetDateTime::parse("2026-10-10T12:00:00Z", &time::format_description::well_known::Rfc3339).unwrap()
    });
    store
}

pub fn owner() -> Principal {
    Principal::solo(OWNER)
}

/// `sha256:<hex>` of a short label, so fixtures can say `"hash": "v1"`.
pub fn hash(text: &str) -> String {
    if text.starts_with("sha256:") {
        text.to_string()
    } else {
        sc_envelope::hash_bytes(text.as_bytes())
    }
}

pub fn location(volume: &str, document: &str, path: &str) -> Location {
    Location {
        volume_uuid: volume.into(),
        document_id: document.into(),
        path: path.into(),
        name: Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()),
        size: None,
        modified_at: None,
    }
}

pub fn audience(labels: &[&str]) -> BTreeSet<String> {
    labels.iter().map(|l| l.to_string()).collect()
}

// ------------------------------------------------------------ scenarios

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub name: String,
    #[serde(default)]
    pub proves: Vec<String>,
    pub steps: Vec<Step>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub enum Step {
    Thing(ThingStep),
    File(FileStep),
    Copy(CopyStep),
    Link(LinkStep),
    Observe(ObserveStep),
    Gone(GoneStep),
    VolumeUnavailable(VolumeStep),
    ExpectInstance(ExpectInstance),
    ExpectState(ExpectState),
    ExpectCount(ExpectCount),
    ExpectDistinct(Vec<String>),
    ExpectSameArtifact(Vec<String>),
    ExpectDistinctArtifacts(Vec<String>),
    ExpectRevisions(ExpectRevisions),
    ExpectArtifactExists(String),
    ExpectThingExists(String),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThingStep {
    #[serde(rename = "as")]
    pub name: String,
    pub label: String,
    #[serde(default)]
    pub audience: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileStep {
    #[serde(rename = "as")]
    pub name: String,
    pub label: String,
    pub volume: String,
    pub document: String,
    pub path: String,
    #[serde(default)]
    pub hash: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub mtime: Option<String>,
    #[serde(default)]
    pub audience: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CopyStep {
    #[serde(rename = "as")]
    pub name: String,
    pub of: String,
    pub volume: String,
    pub document: String,
    pub path: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinkStep {
    #[serde(rename = "as")]
    pub name: String,
    pub source: String,
    pub predicate: String,
    pub target: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObserveStep {
    pub volume: String,
    pub document: String,
    pub path: String,
    #[serde(default)]
    pub hash: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub mtime: Option<String>,
    pub expect: String,
    pub rule: String,
    #[serde(default)]
    pub instance: Option<String>,
    #[serde(default)]
    pub candidates: Option<Vec<String>>,
    #[serde(default)]
    pub content_changed: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoneStep {
    pub instance: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VolumeStep {
    pub volume: String,
    pub instances: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectInstance {
    pub instance: String,
    pub status: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub volume: Option<String>,
    #[serde(default)]
    pub revision_hash: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectState {
    pub relationship: String,
    pub state: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectCount {
    pub entity: String,
    pub relationships: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectRevisions {
    pub artifact: String,
    pub count: usize,
}

pub struct Run {
    pub store: Store,
    pub names: BTreeMap<String, String>,
}

impl Run {
    pub fn id(&self, name: &str) -> String {
        self.names
            .get(name)
            .cloned()
            .unwrap_or_else(|| panic!("scenario names {name:?} before defining it"))
    }

    fn bind(&mut self, name: &str, id: &str) {
        self.names.insert(name.to_string(), id.to_string());
    }
}

pub fn run_scenario(path: &Path) -> Run {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let scenario: Scenario = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut run = Run {
        store: fixed_store(),
        names: BTreeMap::new(),
    };
    let p = owner();
    for (index, step) in scenario.steps.iter().enumerate() {
        let context = format!("{} step {index}: {step:?}", scenario.name);
        match step {
            Step::Thing(step) => {
                let thing = run
                    .store
                    .create_thing(&p, &step.label, step.audience.iter().cloned().collect())
                    .expect(&context);
                run.bind(&step.name, &thing.id);
            }
            Step::File(step) => {
                let mut location = location(&step.volume, &step.document, &step.path);
                location.size = step.size;
                location.modified_at = step.mtime.clone();
                let (artifact, revision, instance) = run
                    .store
                    .register_file(
                        &p,
                        &step.label,
                        location,
                        step.hash.as_deref().map(hash),
                        step.audience.iter().cloned().collect(),
                    )
                    .expect(&context);
                run.bind(&step.name, &instance.id);
                run.bind(&format!("{}.artifact", step.name), &artifact.id);
                run.bind(&format!("{}.revision", step.name), &revision.id);
            }
            Step::Copy(step) => {
                let of = run.store.get_instance(&p, &run.id(&step.of)).expect(&context);
                let instance = run
                    .store
                    .register_instance(
                        &p,
                        &of.revision_id,
                        location(&step.volume, &step.document, &step.path),
                        "local",
                    )
                    .expect(&context);
                run.bind(&step.name, &instance.id);
            }
            Step::Link(step) => {
                let (relationship, assertion) = run
                    .store
                    .link(
                        &p,
                        &step.predicate,
                        &run.id(&step.source),
                        &run.id(&step.target),
                        vec![Evidence::new("fixture", scenario.name.clone())],
                    )
                    .expect(&context);
                run.bind(&step.name, &relationship.id);
                run.bind(&format!("{}.assertion", step.name), &assertion.id);
            }
            Step::Observe(step) => {
                let mut location = location(&step.volume, &step.document, &step.path);
                location.size = step.size;
                location.modified_at = step.mtime.clone();
                let resolution = run
                    .store
                    .observe(
                        &p,
                        Seen {
                            location,
                            content_hash: step.hash.as_deref().map(hash),
                            source: "fixture".into(),
                            window: None,
                        },
                    )
                    .expect(&context);
                assert_eq!(resolution.outcome.as_str(), step.expect, "{context}\n{resolution:#?}");
                assert_eq!(resolution.rule.as_str(), step.rule, "{context}\n{resolution:#?}");
                if let Some(instance) = &step.instance {
                    assert_eq!(resolution.instance_id.as_deref(), Some(run.id(instance).as_str()), "{context}");
                }
                if let Some(candidates) = &step.candidates {
                    let expected: BTreeSet<String> = candidates.iter().map(|c| run.id(c)).collect();
                    let actual: BTreeSet<String> = resolution.candidates.iter().cloned().collect();
                    assert_eq!(actual, expected, "{context}");
                }
                if let Some(changed) = step.content_changed {
                    assert_eq!(resolution.content_changed, changed, "{context}");
                }
                assert!(!resolution.evidence.is_empty(), "{context}: evidence recorded");
            }
            Step::Gone(step) => {
                let resolution = run.store.observe_gone(&p, &run.id(&step.instance), "fixture").expect(&context);
                assert_eq!(resolution.outcome, Outcome::Unresolved, "{context}");
                assert_eq!(resolution.rule, Rule::KnownLocationGone, "{context}");
            }
            Step::VolumeUnavailable(step) => {
                let resolutions = run
                    .store
                    .observe_volume_unavailable(&p, &step.volume, "fixture")
                    .expect(&context);
                assert_eq!(resolutions.len(), step.instances, "{context}");
            }
            Step::ExpectInstance(step) => {
                let instance = run.store.get_instance(&p, &run.id(&step.instance)).expect(&context);
                assert_eq!(instance.status.as_str(), step.status, "{context}\n{instance:#?}");
                if let Some(path) = &step.path {
                    assert_eq!(&instance.location.path, path, "{context}");
                }
                if let Some(volume) = &step.volume {
                    assert_eq!(&instance.location.volume_uuid, volume, "{context}");
                }
                if let Some(expected) = &step.revision_hash {
                    let revision = run.store.get_revision(&p, &instance.revision_id).expect(&context);
                    assert_eq!(revision.content_hash, Some(hash(expected)), "{context}");
                }
            }
            Step::ExpectState(step) => {
                let state = run.store.relationship_state(&p, &run.id(&step.relationship)).expect(&context);
                assert_eq!(state.as_str(), step.state, "{context}");
            }
            Step::ExpectCount(step) => {
                let count = run.store.count_relationships(&p, &run.id(&step.entity)).expect(&context);
                assert_eq!(count, step.relationships, "{context}");
            }
            Step::ExpectDistinct(names) => {
                let ids: BTreeSet<String> = names.iter().map(|n| run.id(n)).collect();
                assert_eq!(ids.len(), names.len(), "{context}");
            }
            Step::ExpectSameArtifact(names) => {
                let artifacts: BTreeSet<String> = names
                    .iter()
                    .map(|n| run.store.get_instance(&p, &run.id(n)).expect(&context).artifact_id)
                    .collect();
                assert_eq!(artifacts.len(), 1, "{context}");
            }
            Step::ExpectDistinctArtifacts(names) => {
                let artifacts: BTreeSet<String> = names
                    .iter()
                    .map(|n| run.store.get_instance(&p, &run.id(n)).expect(&context).artifact_id)
                    .collect();
                assert_eq!(artifacts.len(), names.len(), "{context}");
            }
            Step::ExpectRevisions(step) => {
                let revisions = run.store.revisions_of(&p, &run.id(&step.artifact)).expect(&context);
                assert_eq!(revisions.len(), step.count, "{context}");
            }
            Step::ExpectArtifactExists(name) => {
                run.store.get_artifact(&p, &run.id(name)).expect(&context);
            }
            Step::ExpectThingExists(name) => {
                run.store.get_thing(&p, &run.id(name)).expect(&context);
            }
        }
    }
    assert!(run.store.verify_chain().expect("chain") > 0);
    run
}

/// Run every scenario file in a fixtures subdirectory.
pub fn run_dir(dir: &str) -> usize {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(fixtures_dir().join(dir))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "no scenarios in fixtures/{dir}");
    for path in &paths {
        let run = run_scenario(path);
        // Every scenario's state survives a replay from the log.
        let mut run = run;
        let before = run.store.events(&owner()).unwrap().len();
        let replayed = run.store.replay().unwrap();
        assert_eq!(replayed, before, "{}", path.display());
    }
    paths.len()
}
