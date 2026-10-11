//! Container fixtures (section 10, decision D7) from a static ZIP built at
//! test time from `fixtures/containers/archive-manifest.json`.

mod common;

use common::*;
use sc_core::*;
use serde::Deserialize;
use std::io::{Read, Write};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    name: String,
    #[allow(dead_code)]
    description: String,
    members: Vec<Member>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Member {
    path: String,
    content: String,
}

/// Build the ZIP in a temporary directory and return its path and bytes.
fn build_zip(dir: &std::path::Path) -> (std::path::PathBuf, Vec<u8>, Manifest) {
    let text = std::fs::read_to_string(fixtures_dir().join("containers/archive-manifest.json")).unwrap();
    let manifest: Manifest = serde_json::from_str(&text).unwrap();
    let path = dir.join(&manifest.name);
    let file = std::fs::File::create(&path).unwrap();
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .last_modified_time(zip::DateTime::from_date_and_time(2026, 10, 10, 12, 0, 0).unwrap());
    for member in &manifest.members {
        writer.start_file(&member.path, options).unwrap();
        writer.write_all(member.content.as_bytes()).unwrap();
    }
    writer.finish().unwrap();
    let bytes = std::fs::read(&path).unwrap();
    (path, bytes, manifest)
}

/// Enumerate members the way a container reader would: path, size, checksum.
fn enumerate(bytes: &[u8]) -> Vec<MemberSpec> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut members = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).unwrap();
        let mut content = Vec::new();
        entry.read_to_end(&mut content).unwrap();
        members.push(MemberSpec {
            path: entry.name().to_string(),
            size: content.len() as u64,
            hash: sc_envelope::hash_bytes(&content),
        });
    }
    members
}

struct Mounted {
    store: Store,
    container_revision: String,
    members: Vec<MemberSpec>,
    presentations: Vec<Presentation>,
}

fn mount_all(epoch: u64) -> Mounted {
    let dir = tempfile::tempdir().unwrap();
    let (path, bytes, _) = build_zip(dir.path());
    let mut store = fixed_store();
    let p = owner();
    let (_, container_revision, _) = store
        .register_file(
            &p,
            "household-papers.zip",
            location("vol-robin", "doc-zip", path.to_str().unwrap()),
            Some(sc_envelope::hash_bytes(&bytes)),
            Default::default(),
        )
        .unwrap();
    let members = enumerate(&bytes);
    assert_eq!(members.len(), 3);
    let presentations = members
        .iter()
        .enumerate()
        .map(|(index, member)| {
            store
                .mount_member(
                    &p,
                    &container_revision.id,
                    member.clone(),
                    MountLocator {
                        volume_entity: "peelfs-volume-1".into(),
                        item_id: 2 + index as u64,
                        mount_epoch: epoch,
                    },
                )
                .unwrap()
        })
        .collect();
    Mounted {
        store,
        container_revision: container_revision.id,
        members,
        presentations,
    }
}

#[test]
fn members_get_durable_identity_and_session_scoped_presentations() {
    let mounted = mount_all(1);
    let p = owner();
    let warranty = &mounted.presentations[0];
    assert_eq!(warranty.member.member_path, "Kitchen/Dishwasher warranty.txt");
    assert_eq!(warranty.member.member_hash, mounted.members[0].hash);
    assert_eq!(warranty.status, PresentationStatus::Available);
    assert_eq!(warranty.locator.mount_epoch, 1);
    // The member is an artifact with a revision carrying its checksum; it is not an instance.
    let artifact = mounted.store.get_artifact(&p, &warranty.member_artifact_id).unwrap();
    assert_eq!(artifact.label, "Kitchen/Dishwasher warranty.txt");
    let revision = mounted.store.get_revision(&p, &warranty.member_revision_id).unwrap();
    assert_eq!(revision.content_hash.as_deref(), Some(mounted.members[0].hash.as_str()));
    assert!(mounted.store.instances_of(&p, &artifact.id).unwrap().is_empty());
    let ids: std::collections::BTreeSet<&str> =
        mounted.presentations.iter().map(|pr| pr.member_artifact_id.as_str()).collect();
    assert_eq!(ids.len(), 3);
}

#[test]
fn presentation_lifecycle_unmount_and_remount_rebinds_through_the_member() {
    let mut mounted = mount_all(1);
    let p = owner();
    let before: Vec<String> = mounted.presentations.iter().map(|pr| pr.id.clone()).collect();
    let artifacts_before = count_rows(&mounted.store, "artifacts");
    let instances_before = count_rows(&mounted.store, "instances");

    // Unmount: every presentation becomes unavailable; nothing is created or deleted.
    let unavailable = mounted.store.unmount(&p, "peelfs-volume-1", 1).unwrap();
    assert_eq!(unavailable.len(), 3);
    for presentation in &unavailable {
        assert_eq!(presentation.status, PresentationStatus::Unavailable);
        assert!(presentation.valid_until.is_some());
    }
    assert_eq!(count_rows(&mounted.store, "artifacts"), artifacts_before);
    assert_eq!(count_rows(&mounted.store, "instances"), instances_before);
    assert_eq!(count_rows(&mounted.store, "presentations"), 3);

    // Remount with different item ids and a new epoch: the same presentations
    // come back, bound through the member key, not the item id.
    let mut remounted = Vec::new();
    for (index, member) in mounted.members.iter().enumerate() {
        remounted.push(
            mounted
                .store
                .mount_member(
                    &p,
                    &mounted.container_revision,
                    member.clone(),
                    MountLocator {
                        volume_entity: "peelfs-volume-1".into(),
                        item_id: 40 + index as u64,
                        mount_epoch: 2,
                    },
                )
                .unwrap(),
        );
    }
    let after: Vec<String> = remounted.iter().map(|pr| pr.id.clone()).collect();
    assert_eq!(before, after);
    for presentation in &remounted {
        assert_eq!(presentation.status, PresentationStatus::Available);
        assert_eq!(presentation.locator.mount_epoch, 2);
        assert!(presentation.locator.item_id >= 40);
        assert!(presentation.valid_until.is_none());
    }
    assert_eq!(count_rows(&mounted.store, "artifacts"), artifacts_before);
    assert_eq!(count_rows(&mounted.store, "instances"), instances_before);
    assert_eq!(count_rows(&mounted.store, "presentations"), 3);

    // The history shows mount, unavailable, rebind for one member.
    let ops: Vec<&str> = mounted
        .store
        .history(&p, &before[0])
        .unwrap()
        .iter()
        .map(|e| match e.envelope.body.change {
            Change::PresentationMounted { .. } => "mounted",
            Change::PresentationUnavailable { .. } => "unavailable",
            Change::PresentationRebound { .. } => "rebound",
            _ => "other",
        })
        .collect();
    assert_eq!(ops, ["mounted", "unavailable", "rebound"]);
    mounted.store.verify_chain().unwrap();
}

#[test]
fn extracted_member_is_a_distinct_derived_instance() {
    let mut mounted = mount_all(1);
    let p = owner();
    let warranty = &mounted.presentations[0];
    let instances_before = count_rows(&mounted.store, "instances");
    let (instance, relationship, assertion) = mounted
        .store
        .extract_member(
            &p,
            &warranty.id,
            location("vol-robin", "doc-extracted", "/Volumes/Robin/Kitchen/Dishwasher warranty.txt"),
        )
        .unwrap();
    assert_eq!(count_rows(&mounted.store, "instances"), instances_before + 1);
    assert_eq!(instance.provenance, sc_envelope::ProvenanceKind::Derived);
    assert_eq!(instance.revision_id, warranty.member_revision_id);
    assert_eq!(relationship.predicate, "DERIVED_FROM");
    assert_eq!(relationship.source_id, instance.id);
    assert_eq!(relationship.target_id, warranty.member_revision_id);
    assert_eq!(assertion.status, AssertionStatus::Accepted);
    assert!(assertion.evidence.iter().any(|e| e.kind == "presentation" && e.value == warranty.id));
    assert_eq!(
        mounted.store.relationship_state(&p, &relationship.id).unwrap(),
        RelationshipState::Present
    );
    // The instance event itself carries derived provenance.
    let registered = mounted
        .store
        .history(&p, &instance.id)
        .unwrap()
        .into_iter()
        .find(|e| matches!(e.envelope.body.change, Change::InstanceRegistered { .. }))
        .unwrap();
    assert_eq!(registered.envelope.provenance.kind, sc_envelope::ProvenanceKind::Derived);

    // The presentation is still a presentation, and the extracted instance
    // resolves on its own key afterwards.
    assert_eq!(
        mounted.store.get_presentation(&p, &warranty.id).unwrap().status,
        PresentationStatus::Available
    );
    let resolution = mounted
        .store
        .observe(
            &p,
            Seen {
                location: location("vol-robin", "doc-extracted", "/Volumes/Robin/Desktop/warranty.txt"),
                content_hash: Some(mounted.members[0].hash.clone()),
                source: "scan".into(),
            },
        )
        .unwrap();
    assert_eq!(resolution.outcome, Outcome::Resolved);
    assert_eq!(resolution.instance_id.as_deref(), Some(instance.id.as_str()));

    // Extraction needs an available presentation.
    mounted.store.unmount(&p, "peelfs-volume-1", 1).unwrap();
    let manual = &mounted.presentations[1];
    assert!(matches!(
        mounted
            .store
            .extract_member(&p, &manual.id, location("vol-robin", "doc-x", "/Volumes/Robin/manual.txt")),
        Err(Error::Conflict(_))
    ));
}

fn count_rows(store: &Store, table: &str) -> usize {
    let p = owner();
    // Count through the owner's view so this never bypasses access checks:
    // every row is visible to the owner.
    match table {
        "artifacts" => store.search(&p, "").unwrap().iter().filter(|h| h.kind == "artifact").count(),
        "instances" => store.search(&p, "").unwrap().iter().filter(|h| h.kind == "instance").count(),
        "presentations" => store.search(&p, "").unwrap().iter().filter(|h| h.kind == "presentation").count(),
        _ => unreachable!(),
    }
}
