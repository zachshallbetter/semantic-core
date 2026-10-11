//! Assertions, materialization, authority, retraction, the event chain,
//! replay and crash recovery.

mod common;

use common::*;
use sc_core::*;

fn linked_pair(store: &mut Store) -> (String, String, Relationship, Assertion) {
    let p = owner();
    let thing = store.create_thing(&p, "Dishwasher", Default::default()).unwrap();
    let (artifact, _, _) = store
        .register_file(
            &p,
            "Warranty.pdf",
            location("vol-robin", "doc-1", "/Volumes/Robin/Warranty.pdf"),
            Some(hash("warranty-v1")),
            Default::default(),
        )
        .unwrap();
    let (relationship, assertion) = store
        .link(&p, "DOCUMENTS", &artifact.id, &thing.id, vec![Evidence::new("note", "owner says so")])
        .unwrap();
    (artifact.id, thing.id, relationship, assertion)
}

#[test]
fn relationship_is_not_assertion_and_state_follows_d9() {
    let mut store = fixed_store();
    let p = owner();
    let (_, _, relationship, first) = linked_pair(&mut store);
    assert_eq!(store.relationship_state(&p, &relationship.id).unwrap(), RelationshipState::Present);

    // A second, independent assertion about the same relationship.
    let second = store
        .propose_assertion(&p, &relationship.id, Polarity::Supports, vec![Evidence::new("ocr", "serial matched")])
        .unwrap();
    assert_ne!(first.id, second.id);
    assert_eq!(store.assertions_of(&p, &relationship.id).unwrap().len(), 2);
    store.accept_assertion(&p, &second.id, OWNER_LOCAL).unwrap();

    // Retract one; the relationship remains present with the other's evidence.
    store.retract_assertion(&p, &first.id, "typo").unwrap();
    assert_eq!(store.relationship_state(&p, &relationship.id).unwrap(), RelationshipState::Present);
    let retracted = store.get_assertion(&p, &first.id).unwrap();
    assert_eq!(retracted.status, AssertionStatus::Retracted);
    assert_eq!(retracted.evidence, vec![Evidence::new("note", "owner says so")]);

    // A dispute makes it DISPUTED; retracting the remaining support makes it ABSENT.
    let dispute = store
        .propose_assertion(&p, &relationship.id, Polarity::Disputes, vec![Evidence::new("note", "wrong appliance")])
        .unwrap();
    assert_eq!(store.relationship_state(&p, &relationship.id).unwrap(), RelationshipState::Present);
    store.accept_assertion(&p, &dispute.id, OWNER_LOCAL).unwrap();
    assert_eq!(store.relationship_state(&p, &relationship.id).unwrap(), RelationshipState::Disputed);
    store.retract_assertion(&p, &second.id, "conceded").unwrap();
    assert_eq!(store.relationship_state(&p, &relationship.id).unwrap(), RelationshipState::Absent);
    store.retract_assertion(&p, &dispute.id, "withdrawn").unwrap();
    assert_eq!(store.relationship_state(&p, &relationship.id).unwrap(), RelationshipState::Unknown);

    // Declaring the same triple again returns the same relationship.
    let again = store
        .declare_relationship(&p, "DOCUMENTS", &relationship.source_id, &relationship.target_id)
        .unwrap();
    assert_eq!(again.id, relationship.id);
}

#[test]
fn acceptance_needs_separate_authority_and_qualification_never_promotes() {
    let mut store = fixed_store();
    let p = owner();
    let (artifact, thing, _, _) = linked_pair(&mut store);
    let relationship = store.declare_relationship(&p, "REFERENCES", &artifact, &thing).unwrap();
    let assertion = store
        .propose_assertion(&p, &relationship.id, Polarity::Supports, vec![])
        .unwrap();
    assert_eq!(assertion.status, AssertionStatus::Proposed);
    assert_eq!(store.relationship_state(&p, &relationship.id).unwrap(), RelationshipState::Unknown);

    // Qualification records what the evidence justifies and nothing more.
    let qualified = store.record_qualification(&p, &assertion.id, Qualification::Pass).unwrap();
    assert_eq!(qualified.qualification, Some(Qualification::Pass));
    assert_eq!(qualified.status, AssertionStatus::Proposed);
    assert_eq!(store.relationship_state(&p, &relationship.id).unwrap(), RelationshipState::Unknown);

    // Wrong authority, wrong actor.
    assert!(matches!(
        store.accept_assertion(&p, &assertion.id, "model-guess"),
        Err(Error::Unauthorized(_))
    ));
    let robin = Principal::solo("robin");
    assert!(matches!(
        store.accept_assertion(&robin, &assertion.id, OWNER_LOCAL),
        Err(Error::Unauthorized(_))
    ));
    assert_eq!(store.relationship_state(&p, &relationship.id).unwrap(), RelationshipState::Unknown);

    store.accept_assertion(&p, &assertion.id, OWNER_LOCAL).unwrap();
    assert_eq!(store.relationship_state(&p, &relationship.id).unwrap(), RelationshipState::Present);
    assert!(matches!(
        store.accept_assertion(&p, &assertion.id, OWNER_LOCAL),
        Err(Error::Conflict(_))
    ));
}

#[test]
fn retraction_is_a_compensating_event_that_keeps_history() {
    let mut store = fixed_store();
    let p = owner();
    let (_, _, relationship, assertion) = linked_pair(&mut store);
    let before = store.events(&p).unwrap().len();
    let accept_hash = store
        .history(&p, &assertion.id)
        .unwrap()
        .into_iter()
        .find(|e| matches!(e.envelope.body.change, Change::AssertionAccepted { .. }))
        .unwrap()
        .hash;
    let retraction = store.retract_assertion(&p, &assertion.id, "linked the wrong file").unwrap();
    match &retraction.envelope.body.change {
        Change::AssertionRetracted { compensates, reason, .. } => {
            assert_eq!(compensates, &accept_hash);
            assert_eq!(reason, "linked the wrong file");
        }
        other => panic!("{other:?}"),
    }
    let events = store.events(&p).unwrap();
    assert_eq!(events.len(), before + 1, "nothing was erased");
    let why = store.why(&p, &relationship.id).unwrap();
    assert_eq!(why.state, RelationshipState::Unknown);
    assert_eq!(why.assertions[0].status, AssertionStatus::Retracted);
    assert_eq!(why.assertions[0].evidence.len(), 1);
    let ops: Vec<&str> = why
        .events
        .iter()
        .map(|e| match e.envelope.body.change {
            Change::RelationshipDeclared { .. } => "declared",
            Change::AssertionProposed { .. } => "proposed",
            Change::AssertionAccepted { .. } => "accepted",
            Change::AssertionRetracted { .. } => "retracted",
            _ => "other",
        })
        .collect();
    assert_eq!(ops, ["declared", "proposed", "accepted", "retracted"]);
    assert!(matches!(
        store.retract_assertion(&p, &assertion.id, "again"),
        Err(Error::Conflict(_))
    ));
}

#[test]
fn why_prints_the_rule_that_fired() {
    let mut store = fixed_store();
    let p = owner();
    let thing = store.create_thing(&p, "Dishwasher", Default::default()).unwrap();
    let (_, _, instance) = store
        .register_file(
            &p,
            "Warranty.pdf",
            location("vol-robin", "doc-1", "/Volumes/Robin/Warranty.pdf"),
            Some(hash("warranty-v1")),
            Default::default(),
        )
        .unwrap();
    let (relationship, _) = store.link(&p, "RELATED_TO", &instance.id, &thing.id, vec![]).unwrap();
    store.observe_gone(&p, &instance.id, "scan").unwrap();
    store
        .observe(
            &p,
            Seen {
                location: location("vol-leo", "doc-2", "/Volumes/Leo/Warranty.pdf"),
                content_hash: Some(hash("warranty-v1")),
                source: "scan".into(),
            },
        )
        .unwrap();
    let why = store.why(&p, &relationship.id).unwrap();
    let rules: Vec<Rule> = why.resolutions.iter().map(|r| r.rule).collect();
    assert_eq!(rules, [Rule::KnownLocationGone, Rule::ContentHashSingleCandidatePreviousGone]);
    assert!(why.resolutions[1]
        .evidence
        .iter()
        .any(|e| e.kind == "previous-instance" && e.value.contains("missing")));
}

#[test]
fn vocabulary_rules_apply_on_insert() {
    let mut store = fixed_store();
    let p = owner();
    let (artifact, thing, _, _) = linked_pair(&mut store);
    // Class constraint: a thing cannot DOCUMENT an artifact.
    assert!(matches!(
        store.declare_relationship(&p, "DOCUMENTS", &thing, &artifact),
        Err(Error::Vocab(_))
    ));
    // Extension predicate: accepted and materialized.
    let ext = store.declare_relationship(&p, "x-archive:MEMBER_OF", &artifact, &thing).unwrap();
    assert!(ext.materializes);
    assert_eq!(ext.predicate, "x-archive:MEMBER_OF");
    store.propose_assertion(&p, &ext.id, Polarity::Supports, vec![]).unwrap();
    // Unknown predicate: preserved as a value, never materialized.
    let unknown = store.declare_relationship(&p, "OWNS", &artifact, &thing).unwrap();
    assert_eq!(unknown.predicate, "x-unknown:OWNS");
    assert!(!unknown.materializes);
    let labels: Vec<String> = store
        .relationships_of(&p, &thing)
        .unwrap()
        .into_iter()
        .map(|v| v.label)
        .collect();
    assert_eq!(labels, ["DOCUMENTED_BY", "x-archive:MEMBER_OF"]);
    // Still inspectable by id.
    assert_eq!(store.get_relationship(&p, &unknown.id).unwrap().vocab_version, "sc-vocab-1");
    assert_eq!(store.relationship_state(&p, &unknown.id).unwrap(), RelationshipState::Unknown);
}

#[test]
fn chain_verifies_replays_and_detects_tampering() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.sqlite");
    let (relationship_id, count) = {
        let mut store = Store::open_or_create(&path, OWNER).unwrap();
        let (_, _, relationship, _) = linked_pair(&mut store);
        let count = store.verify_chain().unwrap();
        assert!(count >= 7);
        (relationship.id, count)
    };
    // Reopen: the policy is there, the chain is intact, the state is the same.
    let mut store = Store::open(&path).unwrap();
    let p = owner();
    assert_eq!(store.owner(), OWNER);
    assert_eq!(store.verify_chain().unwrap(), count);
    assert_eq!(store.relationship_state(&p, &relationship_id).unwrap(), RelationshipState::Present);
    assert_eq!(store.replay().unwrap(), count);
    assert_eq!(store.relationship_state(&p, &relationship_id).unwrap(), RelationshipState::Present);
    assert_eq!(store.why(&p, &relationship_id).unwrap().assertions.len(), 1);

    // Each event names its predecessor.
    let events = store.events(&p).unwrap();
    assert!(events[0].envelope.body.previous.is_none());
    for pair in events.windows(2) {
        assert_eq!(pair[1].envelope.body.previous.as_deref(), Some(pair[0].hash.as_str()));
    }
    drop(store);

    // Tamper with a stored envelope: the recomputed hash no longer matches.
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute(
        "UPDATE events SET envelope = replace(envelope, 'Dishwasher', 'Dryer') WHERE seq = 2",
        [],
    )
    .unwrap();
    drop(conn);
    let store = Store::open(&path).unwrap();
    assert!(matches!(store.verify_chain(), Err(Error::ChainBroken { seq: 2, .. })));
}

#[test]
fn crash_recovery_rebuilds_tables_from_the_log() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.sqlite");
    let (relationship_id, instance_id) = {
        let mut store = Store::open_or_create(&path, OWNER).unwrap();
        let (_, _, relationship, _) = linked_pair(&mut store);
        let instance = store.instances_of(&owner(), &relationship.source_id).unwrap().remove(0);
        store.observe_gone(&owner(), &instance.id, "scan").unwrap();
        (relationship.id, instance.id)
    };
    // Simulate a crash that lost the materialized tables but kept the log.
    let conn = rusqlite::Connection::open(&path).unwrap();
    for table in sc_core::schema::MATERIALIZED_TABLES {
        conn.execute(&format!("DELETE FROM {table}"), []).unwrap();
    }
    drop(conn);
    let mut store = Store::open(&path).unwrap();
    let p = owner();
    assert!(matches!(store.get_relationship(&p, &relationship_id), Err(Error::NotFound(_))));
    store.replay().unwrap();
    assert_eq!(store.relationship_state(&p, &relationship_id).unwrap(), RelationshipState::Present);
    assert_eq!(store.get_instance(&p, &instance_id).unwrap().status, InstanceStatus::Missing);

    // A write that fails leaves nothing behind: the next hash still chains
    // to the last committed event.
    let last = store.events(&p).unwrap().last().unwrap().hash.clone();
    assert!(store.declare_relationship(&p, "DOCUMENTS", "missing-id", &instance_id).is_err());
    assert_eq!(store.events(&p).unwrap().last().unwrap().hash, last);
    store.verify_chain().unwrap();
}

#[test]
fn library_owner_is_fixed_and_non_owners_cannot_write() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.sqlite");
    Store::open_or_create(&path, OWNER).unwrap();
    assert!(matches!(Store::open_or_create(&path, "robin"), Err(Error::Unauthorized(_))));
    let mut store = Store::open(&path).unwrap();
    let robin = Principal::solo("robin");
    assert!(matches!(
        store.create_thing(&robin, "Dryer", Default::default()),
        Err(Error::Unauthorized(_))
    ));
    assert!(matches!(store.events(&robin), Err(Error::Unauthorized(_))));
}

#[test]
fn observed_events_carry_a_freshness_window_and_others_do_not() {
    let mut store = fixed_store();
    let p = owner();
    let (_, _, instance) = store
        .register_file(
            &p,
            "Warranty.pdf",
            location("vol-robin", "doc-1", "/Volumes/Robin/Warranty.pdf"),
            Some(hash("warranty-v1")),
            Default::default(),
        )
        .unwrap();
    store.observe_gone(&p, &instance.id, "scan").unwrap();
    let events = store.events(&p).unwrap();
    for event in &events {
        let provenance = &event.envelope.provenance;
        let observed = matches!(event.envelope.body.change, Change::ObservationRecorded { .. });
        assert_eq!(provenance.kind == sc_envelope::ProvenanceKind::Observed, observed);
        assert_eq!(provenance.observed_at.is_some(), observed);
        assert_eq!(provenance.expires_at.is_some(), observed);
        event.envelope.validate().unwrap();
    }
    // An expired observation is kept and reported stale, not dropped.
    let observation = events
        .iter()
        .find(|e| matches!(e.envelope.body.change, Change::ObservationRecorded { .. }))
        .unwrap();
    let later = time::OffsetDateTime::parse("2026-10-10T14:00:00Z", &time::format_description::well_known::Rfc3339)
        .unwrap();
    assert_eq!(
        observation.envelope.require_fresh_at(later),
        Err(sc_envelope::FreshnessError::Expired)
    );
    assert_eq!(store.events(&p).unwrap().len(), events.len());
}
