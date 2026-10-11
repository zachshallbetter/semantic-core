//! Signed export and import: round trip, idempotence, referential integrity,
//! tampered signatures, rotated keys, per-actor keys, the acceptance gate,
//! policy refusal, owner re-basing, projected audiences, preserved times,
//! trust class unchanged by a signature.

mod common;

use common::*;
use ed25519_dalek::SigningKey;
use sc_core::*;

fn key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

fn trust(actor: &str, key_id: &str, signing: &SigningKey) -> TrustedKeys {
    TrustedKeys::new().trust(actor, key_id, signing.verifying_key())
}

fn populated() -> (Store, String, String, String) {
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
    let (relationship, assertion) = store
        .link(&p, "DOCUMENTS", &artifact.id, &thing.id, vec![Evidence::new("note", "receipt")])
        .unwrap();
    store.observe_gone(&p, &instance.id, "scan").unwrap();
    (store, relationship.id, instance.id, assertion.id)
}

#[test]
fn export_import_round_trip_is_idempotent_and_keeps_history() {
    let (source, relationship_id, instance_id, assertion_id) = populated();
    let p = owner();
    let signing = key(3);
    let export = source.export(&p, "ava-key:2026", &signing).unwrap();
    assert_eq!(export.format, INTERCHANGE_FORMAT);
    assert!(!export.events.is_empty());
    let text = serde_json::to_string_pretty(&export).unwrap();
    let parsed: Interchange = serde_json::from_str(&text).unwrap();
    let keys = trust(OWNER, "ava-key:2026", &signing);

    let mut target = fixed_store();
    let first = target.import(&p, &parsed, &keys, None).unwrap();
    assert_eq!(first.applied + first.acceptances_withheld, export.events.len());
    assert_eq!(first.acceptances_withheld, 1);
    assert_eq!(first.skipped, 0);
    assert_eq!(first.rebased, 0);
    // The acceptance did not travel: the assertion arrives as a proposal.
    assert_eq!(target.relationship_state(&p, &relationship_id).unwrap(), RelationshipState::Unknown);
    assert_eq!(target.get_assertion(&p, &assertion_id).unwrap().status, AssertionStatus::Proposed);
    assert_eq!(target.get_instance(&p, &instance_id).unwrap().status, InstanceStatus::Missing);
    let why = target.why(&p, &relationship_id).unwrap();
    assert_eq!(why.assertions[0].evidence, vec![Evidence::new("note", "receipt")]);
    assert_eq!(why.events.len(), 2);
    target.verify_chain().unwrap();
    // The local authority accepts it through the ordinary path.
    target.accept_assertion(&p, &assertion_id, OWNER_LOCAL).unwrap();
    assert_eq!(target.relationship_state(&p, &relationship_id).unwrap(), RelationshipState::Present);

    // Importing again changes nothing.
    let second = target.import(&p, &parsed, &keys, None).unwrap();
    assert_eq!(second.applied, 0);
    assert_eq!(second.skipped + second.acceptances_withheld, export.events.len());
    assert_eq!(second.acceptances_withheld, 1);
    let events_after = target.events(&p).unwrap().len();
    // Imported events remember their origin and when they arrived; their
    // `at` is the source's time.
    for event in target.events(&p).unwrap().iter().skip(1) {
        if event.envelope.body.origin.is_some() {
            assert_eq!(event.envelope.body.at, "2026-10-10T12:00:00Z");
            assert!(event.envelope.body.imported_at.is_some());
        }
    }

    // Re-exporting the target and importing into the source is also a no-op.
    let back = target.export(&p, "ava-key:2026", &signing).unwrap();
    let mut source = source;
    let report = source.import(&p, &back, &keys, None).unwrap();
    assert_eq!(report.applied, 0);
    assert_eq!(target.events(&p).unwrap().len(), events_after);
}

#[test]
fn source_event_time_survives_import() {
    let (source, _, _, _) = populated();
    let p = owner();
    let signing = key(3);
    let export = source.export(&p, "k", &signing).unwrap();
    let mut target = Store::in_memory(OWNER).unwrap();
    target.set_clock(|| {
        time::OffsetDateTime::parse("2026-11-01T09:00:00Z", &time::format_description::well_known::Rfc3339).unwrap()
    });
    target.import(&p, &export, &trust(OWNER, "k", &signing), None).unwrap();
    let imported: Vec<Event> = target
        .events(&p)
        .unwrap()
        .into_iter()
        .filter(|e| e.envelope.body.origin.is_some())
        .collect();
    assert!(!imported.is_empty());
    for event in imported {
        assert_eq!(event.envelope.body.at, "2026-10-10T12:00:00Z");
        assert_eq!(event.envelope.body.imported_at.as_deref(), Some("2026-11-01T09:00:00Z"));
    }
}

#[test]
fn import_verifies_every_signature_before_applying_anything() {
    let (source, _, _, _) = populated();
    let p = owner();
    let signing = key(3);
    let export = source.export(&p, "ava-key:2026", &signing).unwrap();
    let keys = trust(OWNER, "ava-key:2026", &signing);

    // Tampered content under a valid-looking signature.
    let mut tampered = export.clone();
    let last = tampered.events.len() - 1;
    tampered.events[last].envelope.body.at = "2026-10-10T12:00:01Z".into();
    let mut target = fixed_store();
    assert!(matches!(target.import(&p, &tampered, &keys, None), Err(Error::Interchange(_))));
    assert_eq!(target.events(&p).unwrap().len(), 1, "nothing was applied");

    // Rotated key: the file signed by the old key does not verify under the new one.
    let rotated = key(4);
    assert!(matches!(
        target.import(&p, &export, &trust(OWNER, "ava-key:2026", &rotated), None),
        Err(Error::Interchange(_))
    ));
    // Re-signed under the rotated key it imports, and the ids are unchanged.
    let resigned = source.export(&p, "ava-key:2027", &rotated).unwrap();
    target.import(&p, &resigned, &trust(OWNER, "ava-key:2027", &rotated), None).unwrap();
    for hit in source.search(&p, "").unwrap() {
        target.kind_of(&p, &hit.id).unwrap();
    }
}

#[test]
fn keys_are_bound_to_actors() {
    let (source, _, _, _) = populated();
    let p = owner();
    let signing = key(3);
    let export = source.export(&p, "ava-key", &signing).unwrap();
    let mut target = fixed_store();
    // The right key, trusted for a different actor: refused.
    let wrong_actor = trust("robin", "ava-key", &signing);
    let error = target.import(&p, &export, &wrong_actor, None).unwrap_err();
    assert!(error.to_string().contains("no trusted key for actor"), "{error}");
    // Two actors in one file, each with its own key; one event forged with
    // the other actor's key is refused whole.
    let robin_key = key(6);
    let mut mixed = export.clone();
    let mut forged = mixed.events[0].clone();
    forged.envelope.body.actor = "robin".into();
    forged = forged.envelope.sign("ava-key", &signing).unwrap();
    mixed.events.push(forged);
    let both = TrustedKeys::new()
        .trust(OWNER, "ava-key", signing.verifying_key())
        .trust("robin", "robin-key", robin_key.verifying_key());
    let error = target.import(&p, &mixed, &both, None).unwrap_err();
    assert!(error.to_string().contains("by robin"), "{error}");
    assert_eq!(target.events(&p).unwrap().len(), 1);
    // Nothing trusted at all: refused.
    assert!(target.import(&p, &export, &TrustedKeys::new(), None).is_err());
}

#[test]
fn policy_events_are_refused_whole() {
    let (source, _, _, _) = populated();
    let p = owner();
    let signing = key(3);
    let mut export = source.export(&p, "k", &signing).unwrap();
    let mut policy = export.events[0].clone();
    policy.envelope.body.change = Change::PolicyEstablished {
        authority: "owner-local".into(),
        owner: "hale".into(),
    };
    policy.envelope.subject_id = "sc:policy:default".into();
    let policy = policy.envelope.sign("k", &signing).unwrap();
    export.events.push(policy);
    let mut target = fixed_store();
    let error = target.import(&p, &export, &trust(OWNER, "k", &signing), None).unwrap_err();
    assert!(error.to_string().contains("policy"), "{error}");
    assert_eq!(target.events(&p).unwrap().len(), 1, "nothing before the policy event was applied either");
    assert_eq!(target.owner(), OWNER);
}

#[test]
fn foreign_acceptance_arrives_as_a_proposal() {
    let (source, relationship_id, _, assertion_id) = populated();
    let p = owner();
    let signing = key(3);
    let export = source.export(&p, "k", &signing).unwrap();
    // The source accepted under its own authority; make the name foreign.
    let mut foreign = export.clone();
    for signed in &mut foreign.events {
        if let Change::AssertionAccepted { authority, .. } = &mut signed.envelope.body.change {
            *authority = "household-of-hale".into();
        }
        signed.envelope.body.authority = "household-of-hale".into();
        *signed = signed.envelope.sign("k", &signing).unwrap();
    }
    let mut target = fixed_store();
    let report = target.import(&p, &foreign, &trust(OWNER, "k", &signing), None).unwrap();
    assert_eq!(report.acceptances_withheld, 1);
    let assertion = target.get_assertion(&p, &assertion_id).unwrap();
    assert_eq!(assertion.status, AssertionStatus::Proposed);
    assert_eq!(assertion.accepted_by, None);
    assert_eq!(target.relationship_state(&p, &relationship_id).unwrap(), RelationshipState::Unknown);
    // The imported record keeps the source authority name; nothing rewrote it.
    let proposed = target
        .events(&p)
        .unwrap()
        .into_iter()
        .find(|e| matches!(e.envelope.body.change, Change::AssertionProposed { .. }))
        .unwrap();
    assert_eq!(proposed.envelope.body.authority, "household-of-hale");
    // Nothing in the target's log carries an acceptance under a foreign name.
    assert!(!target.events(&p).unwrap().iter().any(|e| matches!(
        &e.envelope.body.change,
        Change::AssertionAccepted { authority, .. } if authority != OWNER_LOCAL
    )));
}

#[test]
fn different_owners_rebase_onto_the_importer_and_record_the_source_owner() {
    let mut robin_library = Store::in_memory("robin").unwrap();
    let robin = Principal::solo("robin");
    let thing = robin_library.create_thing(&robin, "Lawn mower", audience(&["adults"])).unwrap();
    let (artifact, _, _) = robin_library
        .register_file(
            &robin,
            "Mower manual.pdf",
            location("vol-robin", "doc-1", "/Volumes/Robin/Mower manual.pdf"),
            Some(hash("mower")),
            audience(&["adults", "kids"]),
        )
        .unwrap();
    robin_library.link(&robin, "DOCUMENTS", &artifact.id, &thing.id, vec![]).unwrap();
    let robin_key = key(8);
    let export = robin_library.export(&robin, "robin-key", &robin_key).unwrap();
    assert_eq!(export.exporter, "robin");
    // Access blocks in the file still name the source owner; audiences are projected.
    for signed in &export.events {
        for owner_name in signed.envelope.body.change.owners() {
            assert_eq!(owner_name, "robin");
        }
    }

    let mut ava_library = fixed_store();
    let ava = owner();
    let keys = TrustedKeys::new().trust("robin", "robin-key", robin_key.verifying_key());
    // Without adopting the owner the file is refused whole; nothing lands.
    let refused = ava_library.import(&ava, &export, &keys, None).unwrap_err();
    assert!(refused.to_string().contains("adopt-owner"), "{refused}");
    assert_eq!(ava_library.events(&ava).unwrap().len(), 1);
    let wrong = ava_library.import(&ava, &export, &keys, Some("hale")).unwrap_err();
    assert!(matches!(wrong, Error::Interchange(_)), "{wrong}");
    assert_eq!(ava_library.events(&ava).unwrap().len(), 1);
    let report = ava_library.import(&ava, &export, &keys, Some("robin")).unwrap();
    assert!(report.rebased >= 4, "{report:?}");
    // Everything imported is owned by the importer, and the source owner is on the event.
    let imported_thing = ava_library.get_thing(&ava, &thing.id).unwrap();
    assert_eq!(imported_thing.access.owner, OWNER);
    assert_eq!(imported_thing.access.audience, audience(&["member"]));
    let event = ava_library
        .events(&ava)
        .unwrap()
        .into_iter()
        .find(|e| matches!(e.envelope.body.change, Change::ThingCreated { .. }))
        .unwrap();
    assert_eq!(event.envelope.body.source_owner.as_deref(), Some("robin"));
    assert_eq!(event.envelope.body.actor, "robin");
    // Robin, as a principal in Ava's library, is not an owner there.
    assert!(matches!(
        ava_library.get_thing(&robin, &thing.id),
        Err(Error::NotFound(_))
    ));
    assert!(ava_library
        .get_thing(&Principal::new("leo", ["member"]), &thing.id)
        .is_ok());
    ava_library.verify_chain().unwrap();
    ava_library.replay().unwrap();
    assert_eq!(ava_library.get_thing(&ava, &thing.id).unwrap().access.owner, OWNER);
}

#[test]
fn export_projects_audiences_and_never_leaks_raw_labels() {
    let (source, _, _, _) = populated();
    let p = owner();
    let export = source.export(&p, "k", &key(3)).unwrap();
    let text = serde_json::to_string(&export).unwrap();
    assert!(!text.contains("adults"), "raw audience label left the device");
    for signed in &export.events {
        assert_eq!(signed.envelope.minimum_access, sc_envelope::AccessLevel::Member);
        assert!(signed.envelope.body.previous.is_none(), "local chain position does not travel");
        assert!(signed.envelope.body.origin.is_some());
    }
    // The library itself still holds the raw labels.
    let hits = source.search(&p, "Dishwasher").unwrap();
    assert_eq!(source.get_thing(&p, &hits[0].id).unwrap().access.audience, audience(&["adults"]));
}

#[test]
fn import_preserves_referential_integrity() {
    let (source, _, _, _) = populated();
    let p = owner();
    let signing = key(3);
    let mut export = source.export(&p, "ava-key:2026", &signing).unwrap();
    export.events.retain(|e| {
        !matches!(
            e.envelope.body.change,
            Change::ThingCreated { .. } | Change::ArtifactCreated { .. }
        )
    });
    let mut target = fixed_store();
    let error = target.import(&p, &export, &trust(OWNER, "ava-key:2026", &signing), None).unwrap_err();
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
    let signing = key(5);
    let export = source.export(&p, "ava-key", &signing).unwrap();
    for signed in &export.events {
        signed.verify("ava-key", &signing.verifying_key()).unwrap();
    }
    let mut target = fixed_store();
    target.import(&p, &export, &trust(OWNER, "ava-key", &signing), None).unwrap();
    let imported = target.get_instance(&p, &external.id).unwrap();
    assert_eq!(imported.trust_class, "external");
    assert_eq!(imported.provenance, sc_envelope::ProvenanceKind::Declared);
}

#[test]
fn export_is_the_only_place_signatures_appear() {
    let (source, _, _, _) = populated();
    let p = owner();
    for event in source.events(&p).unwrap() {
        let json = serde_json::to_value(&event.envelope).unwrap();
        assert!(json.get("authentication").is_none());
    }
    let signing = key(3);
    let export = source.export(&p, "ava-key:2026", &signing).unwrap();
    for signed in &export.events {
        assert_eq!(signed.authentication.key_id, "ava-key:2026");
    }
    let mut target = fixed_store();
    assert!(matches!(
        target.import(&Principal::solo("robin"), &export, &trust(OWNER, "ava-key:2026", &signing), None),
        Err(Error::Unauthorized(_))
    ));
}
