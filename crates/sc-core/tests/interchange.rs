//! Signed export and import: round trip, idempotence, referential integrity,
//! tampered signatures, rotated keys, trust class unchanged by a signature.

mod common;

use common::*;
use ed25519_dalek::SigningKey;
use sc_core::*;

fn populated() -> (Store, String, String) {
    let mut store = fixed_store();
    let p = owner();
    let thing = store.create_thing(&p, "Dishwasher", audience(&["adults"])).unwrap();
    let (artifact, _, instance) = store
        .register_file(
            &p,
            "Warranty.pdf",
            location("vol-robin", "doc-1", "/Volumes/Robin/Warranty.pdf"),
            Some(hash("warranty-v1")),
            audience(&["adults"]),
        )
        .unwrap();
    let (relationship, _) = store
        .link(&p, "DOCUMENTS", &artifact.id, &thing.id, vec![Evidence::new("note", "receipt")])
        .unwrap();
    store.observe_gone(&p, &instance.id, "scan").unwrap();
    (store, relationship.id, instance.id)
}

#[test]
fn export_import_round_trip_is_idempotent_and_keeps_history() {
    let (source, relationship_id, instance_id) = populated();
    let p = owner();
    let key = SigningKey::from_bytes(&[3_u8; 32]);
    let export = source.export(&p, "ava-key:2026", &key).unwrap();
    assert_eq!(export.format, INTERCHANGE_FORMAT);
    assert!(!export.events.is_empty());
    let text = serde_json::to_string_pretty(&export).unwrap();
    let parsed: Interchange = serde_json::from_str(&text).unwrap();

    let mut target = fixed_store();
    let first = target.import(&p, &parsed, &key.verifying_key()).unwrap();
    assert_eq!(first.applied, export.events.len());
    assert_eq!(first.skipped, 0);
    assert_eq!(target.relationship_state(&p, &relationship_id).unwrap(), RelationshipState::Present);
    assert_eq!(target.get_instance(&p, &instance_id).unwrap().status, InstanceStatus::Missing);
    let why = target.why(&p, &relationship_id).unwrap();
    assert_eq!(why.assertions[0].evidence, vec![Evidence::new("note", "receipt")]);
    assert_eq!(why.events.len(), 3);
    target.verify_chain().unwrap();

    // Importing again changes nothing.
    let second = target.import(&p, &parsed, &key.verifying_key()).unwrap();
    assert_eq!(second.applied, 0);
    assert_eq!(second.skipped, export.events.len());
    let events_after = target.events(&p).unwrap().len();
    // Imported events remember their origin.
    assert!(target
        .events(&p)
        .unwrap()
        .iter()
        .skip(1)
        .all(|e| e.envelope.body.origin.is_some()));

    // Re-exporting the target and importing into the source is also a no-op.
    let back = target.export(&p, "ava-key:2026", &key).unwrap();
    let mut source = source;
    let report = source.import(&p, &back, &key.verifying_key()).unwrap();
    assert_eq!(report.applied, 0);
    assert_eq!(target.events(&p).unwrap().len(), events_after);
}

#[test]
fn import_verifies_every_signature_before_applying_anything() {
    let (source, _, _) = populated();
    let p = owner();
    let key = SigningKey::from_bytes(&[3_u8; 32]);
    let export = source.export(&p, "ava-key:2026", &key).unwrap();

    // Tampered content under a valid-looking signature.
    let mut tampered = export.clone();
    let last = tampered.events.len() - 1;
    tampered.events[last].envelope.body.actor = "mallet".into();
    let mut target = fixed_store();
    assert!(matches!(
        target.import(&p, &tampered, &key.verifying_key()),
        Err(Error::Interchange(_))
    ));
    assert_eq!(target.events(&p).unwrap().len(), 1, "nothing was applied");

    // Rotated key: the file signed by the old key does not verify under the new one.
    let rotated = SigningKey::from_bytes(&[4_u8; 32]);
    assert!(matches!(
        target.import(&p, &export, &rotated.verifying_key()),
        Err(Error::Interchange(_))
    ));
    // Re-signed under the rotated key it imports, and the ids are unchanged.
    let resigned = source.export(&p, "ava-key:2027", &rotated).unwrap();
    target.import(&p, &resigned, &rotated.verifying_key()).unwrap();
    let ids: Vec<String> = source
        .search(&p, "")
        .unwrap()
        .into_iter()
        .map(|h| h.id)
        .collect();
    for id in ids {
        target.kind_of(&p, &id).unwrap();
    }
}

#[test]
fn import_preserves_referential_integrity() {
    let (source, _, _) = populated();
    let p = owner();
    let key = SigningKey::from_bytes(&[3_u8; 32]);
    let mut export = source.export(&p, "ava-key:2026", &key).unwrap();
    // Drop the events that create the thing and the artifact; the relationship
    // now points at identifiers the target cannot find.
    export.events.retain(|e| {
        !matches!(
            e.envelope.body.change,
            Change::ThingCreated { .. } | Change::ArtifactCreated { .. }
        )
    });
    let mut target = fixed_store();
    let error = target.import(&p, &export, &key.verifying_key()).unwrap_err();
    assert!(matches!(error, Error::Interchange(_)), "{error}");
    assert!(error.to_string().contains("references"));
    assert_eq!(target.events(&p).unwrap().len(), 1, "all or nothing");
    assert!(target.search(&p, "").unwrap().is_empty());
}

#[test]
fn a_verified_signature_does_not_change_trust_class() {
    let mut source = fixed_store();
    let p = owner();
    let (_, revision, _) = source
        .register_file(
            &p,
            "Vendor firmware notes.pdf",
            location("vol-robin", "doc-1", "/Volumes/Robin/Downloads/firmware.pdf"),
            Some(hash("firmware")),
            Default::default(),
        )
        .unwrap();
    let external = source
        .register_instance(
            &p,
            &revision.id,
            location("vol-usb", "doc-7", "/Volumes/USB/firmware.pdf"),
            "external",
        )
        .unwrap();
    let key = SigningKey::from_bytes(&[5_u8; 32]);
    let export = source.export(&p, "ava-key", &key).unwrap();
    for signed in &export.events {
        signed.verify("ava-key", &key.verifying_key()).unwrap();
    }
    let mut target = fixed_store();
    target.import(&p, &export, &key.verifying_key()).unwrap();
    let imported = target.get_instance(&p, &external.id).unwrap();
    assert_eq!(imported.trust_class, "external");
    assert_eq!(imported.provenance, sc_envelope::ProvenanceKind::Declared);
}

#[test]
fn export_is_the_only_place_signatures_appear() {
    let (source, _, _) = populated();
    let p = owner();
    // The local log holds unsigned envelopes (D4).
    for event in source.events(&p).unwrap() {
        let json = serde_json::to_value(&event.envelope).unwrap();
        assert!(json.get("authentication").is_none());
    }
    let key = SigningKey::from_bytes(&[3_u8; 32]);
    let export = source.export(&p, "ava-key:2026", &key).unwrap();
    for signed in &export.events {
        assert_eq!(signed.authentication.key_id, "ava-key:2026");
        assert_eq!(signed.envelope.minimum_access, sc_envelope::AccessLevel::Member);
    }
    // A non-owner cannot import.
    let mut target = fixed_store();
    assert!(matches!(
        target.import(&Principal::solo("robin"), &export, &key.verifying_key()),
        Err(Error::Unauthorized(_))
    ));
}
