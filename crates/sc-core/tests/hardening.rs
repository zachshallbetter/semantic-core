//! Correctness fixes: chain forks under concurrent writers, per-row access
//! on collection reads, unregistered predicates, existence leaks through
//! counts and sequence numbers, library opening, and the integer bound that
//! keeps canonical JSON within RFC 8785.

mod common;

use common::*;
use ed25519_dalek::SigningKey;
use sc_core::*;

// ------------------------------------------------------------------ #2

#[test]
fn two_writers_on_one_library_never_fork_the_chain() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.sqlite");
    Store::open_or_create(&path, OWNER).unwrap();
    let writers: Vec<_> = (0..2)
        .map(|w| {
            let path = path.clone();
            std::thread::spawn(move || {
                let mut store = Store::open(&path).unwrap();
                let p = owner();
                for i in 0..25 {
                    store.create_thing(&p, &format!("writer {w} thing {i}"), Default::default()).unwrap();
                }
            })
        })
        .collect();
    for writer in writers {
        writer.join().unwrap();
    }
    let store = Store::open(&path).unwrap();
    assert_eq!(store.verify_chain().unwrap(), 51);
    let events = store.events(&owner()).unwrap();
    let mut predecessors: Vec<Option<String>> = events.iter().map(|e| e.envelope.body.previous.clone()).collect();
    predecessors.sort();
    predecessors.dedup();
    assert_eq!(predecessors.len(), 51, "every event has a distinct predecessor");
}

#[test]
fn a_second_successor_for_one_predecessor_is_refused_by_the_schema() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.sqlite");
    let (hash, envelope) = {
        let mut store = Store::open_or_create(&path, OWNER).unwrap();
        store.create_thing(&owner(), "Dishwasher", Default::default()).unwrap();
        let events = store.events(&owner()).unwrap();
        let last = events.last().unwrap();
        (last.hash.clone(), serde_json::to_string(&last.envelope).unwrap())
    };
    // Simulate a writer that read the same predecessor and tries to append.
    let conn = rusqlite::Connection::open(&path).unwrap();
    let previous: Option<String> = conn
        .query_row("SELECT previous FROM events WHERE hash = ?1", [&hash], |r| r.get(0))
        .unwrap();
    let result = conn.execute(
        "INSERT INTO events (hash, previous, origin, subject, at, envelope) VALUES ('sha256:fork', ?1, NULL, 's', 't', ?2)",
        rusqlite::params![previous, envelope],
    );
    assert!(result.is_err(), "UNIQUE(previous) must refuse the fork");
    drop(conn);
    Store::open(&path).unwrap().verify_chain().unwrap();
}

// ------------------------------------------------------------------ #3

#[test]
fn collection_reads_apply_the_row_rule_after_a_child_is_narrowed() {
    let mut store = fixed_store();
    let p = owner();
    let robin = Principal::new("robin", ["adults"]);
    let thing = store.create_thing(&p, "Dishwasher", audience(&["adults"])).unwrap();
    let (artifact, revision, first) = store
        .register_file(
            &p,
            "Manual.pdf",
            location("vol-robin", "doc-1", "/Volumes/Robin/Manual.pdf"),
            Some(hash("manual")),
            audience(&["adults"]),
        )
        .unwrap();
    let second = store
        .register_instance(&p, &revision.id, location("vol-robin", "doc-2", "/Volumes/Robin/Desktop/Manual.pdf"), "local")
        .unwrap();
    let other_revision = store.create_revision(&p, &artifact.id, Some(hash("manual-v2")), None).unwrap();
    let (relationship, accepted) = store.link(&p, "DOCUMENTS", &artifact.id, &thing.id, vec![]).unwrap();
    let proposed = store
        .propose_assertion(&p, &relationship.id, Polarity::Supports, vec![])
        .unwrap();
    // An ambiguous sighting that names both instances.
    store.observe_gone(&p, &first.id, "scan").unwrap();
    store.observe_gone(&p, &second.id, "scan").unwrap();
    let ambiguous = store
        .observe(
            &p,
            Seen {
                location: location("vol-leo", "doc-9", "/Volumes/Leo/Manual.pdf"),
                content_hash: Some(hash("manual")),
                source: "scan".into(),
            },
        )
        .unwrap();
    assert_eq!(ambiguous.outcome, Outcome::Ambiguous);

    // Before narrowing, robin sees everything under the artifact.
    assert_eq!(store.instances_of(&robin, &artifact.id).unwrap().len(), 2);
    assert_eq!(store.revisions_of(&robin, &artifact.id).unwrap().len(), 2);
    assert_eq!(store.assertions_of(&robin, &relationship.id).unwrap().len(), 2);
    assert!(store
        .resolutions_for_instance(&robin, &first.id)
        .unwrap()
        .iter()
        .any(|r| r.id == ambiguous.id));

    // Narrow one child of each kind to owner-only.
    store.set_audience(&p, &second.id, Default::default()).unwrap();
    store.set_audience(&p, &other_revision.id, Default::default()).unwrap();
    store.set_audience(&p, &proposed.id, Default::default()).unwrap();

    let instances = store.instances_of(&robin, &artifact.id).unwrap();
    assert_eq!(instances.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), [first.id.as_str()]);
    let revisions = store.revisions_of(&robin, &artifact.id).unwrap();
    assert_eq!(revisions.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), [revision.id.as_str()]);
    let assertions = store.assertions_of(&robin, &relationship.id).unwrap();
    assert_eq!(assertions.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), [accepted.id.as_str()]);
    // The ambiguous resolution names the narrowed instance, so robin no
    // longer receives it from the visible instance's side either.
    assert!(!store
        .resolutions_for_instance(&robin, &first.id)
        .unwrap()
        .iter()
        .any(|r| r.id == ambiguous.id));
    assert!(matches!(store.get_instance(&robin, &second.id), Err(Error::NotFound(_))));
    // The owner still sees all of it, and inspect follows the same rule.
    assert_eq!(store.instances_of(&p, &artifact.id).unwrap().len(), 2);
    match store.inspect(&robin, &artifact.id).unwrap() {
        Inspection::Artifact { instances, revisions, .. } => {
            assert_eq!(instances.len(), 1);
            assert_eq!(revisions.len(), 1);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn presentations_of_a_container_apply_the_row_rule() {
    let mut store = fixed_store();
    let p = owner();
    let robin = Principal::new("robin", ["adults"]);
    let (_, container, _) = store
        .register_file(
            &p,
            "papers.zip",
            location("vol-robin", "doc-zip", "/Volumes/Robin/papers.zip"),
            Some(hash("zip")),
            audience(&["adults"]),
        )
        .unwrap();
    let mut presentations = Vec::new();
    for (i, name) in ["a.txt", "b.txt"].iter().enumerate() {
        presentations.push(
            store
                .mount_member(
                    &p,
                    &container.id,
                    MemberSpec {
                        path: name.to_string(),
                        size: 1,
                        hash: hash(name),
                    },
                    MountLocator {
                        volume_entity: "vol".into(),
                        item_id: 2 + i as u64,
                        mount_epoch: 1,
                    },
                )
                .unwrap(),
        );
    }
    assert_eq!(store.presentations_of_container(&robin, &container.id).unwrap().len(), 2);
    store.set_audience(&p, &presentations[1].id, Default::default()).unwrap();
    let visible = store.presentations_of_container(&robin, &container.id).unwrap();
    assert_eq!(visible.iter().map(|pr| pr.id.as_str()).collect::<Vec<_>>(), [presentations[0].id.as_str()]);
    assert_eq!(store.presentations_of_container(&p, &container.id).unwrap().len(), 2);
}

// ------------------------------------------------------------------ #5

#[test]
fn unregistered_predicates_are_refused_locally_and_inert_when_imported() {
    let mut source = fixed_store();
    let p = owner();
    let thing = source.create_thing(&p, "Dishwasher", Default::default()).unwrap();
    let (artifact, _, _) = source
        .register_file(
            &p,
            "Manual.pdf",
            location("vol-robin", "doc-1", "/Volumes/Robin/Manual.pdf"),
            Some(hash("manual")),
            Default::default(),
        )
        .unwrap();
    let error = source.declare_relationship(&p, "OWNS", &artifact.id, &thing.id).unwrap_err();
    assert!(matches!(error, Error::Vocab(sc_vocab::VocabError::Unregistered(_))), "{error}");
    assert!(error.to_string().contains("x-<namespace>:NAME"));
    assert!(source.relationships_of(&p, &thing.id).unwrap().is_empty());

    // An unknown predicate written by another library arrives through
    // import, preserved as a value.
    let (relationship, _) = source.link(&p, "REFERENCES", &artifact.id, &thing.id, vec![]).unwrap();
    let signing = SigningKey::from_bytes(&[2; 32]);
    let mut export = source.export(&p, "k", &signing).unwrap();
    for signed in &mut export.events {
        if let Change::RelationshipDeclared { relationship } = &mut signed.envelope.body.change {
            relationship.predicate = "x-unknown:OWNS".into();
            relationship.materializes = false;
            *signed = signed.envelope.sign("k", &signing).unwrap();
        }
    }
    let mut target = fixed_store();
    target
        .import(&p, &export, &TrustedKeys::new().trust(OWNER, "k", signing.verifying_key()))
        .unwrap();
    let imported = target.get_relationship(&p, &relationship.id).unwrap();
    assert_eq!(imported.predicate, "x-unknown:OWNS");
    assert!(!imported.materializes);
    // The file also carried a proposed assertion about it; state stays UNKNOWN.
    assert_eq!(target.assertions_of(&p, &relationship.id).unwrap().len(), 1);
    assert_eq!(target.relationship_state(&p, &relationship.id).unwrap(), RelationshipState::Unknown);
    assert!(target.relationships_of(&p, &thing.id).unwrap().is_empty());
    let assertion_id = target.assertions_of(&p, &relationship.id).unwrap()[0].id.clone();
    assert!(matches!(
        target.accept_assertion(&p, &assertion_id, OWNER_LOCAL),
        Err(Error::Conflict(_))
    ));
    assert!(matches!(
        target.propose_assertion(&p, &relationship.id, Polarity::Supports, vec![]),
        Err(Error::Conflict(_))
    ));
    assert_eq!(target.relationship_state(&p, &relationship.id).unwrap(), RelationshipState::Unknown);
    target.replay().unwrap();
    assert_eq!(target.relationship_state(&p, &relationship.id).unwrap(), RelationshipState::Unknown);
}

// ------------------------------------------------------------------ #6

#[test]
fn restricted_principals_learn_neither_counts_nor_gaps() {
    let mut store = fixed_store();
    let p = owner();
    let robin = Principal::new("robin", ["adults"]);
    let shared = store.create_thing(&p, "Dishwasher", audience(&["adults"])).unwrap();
    let (manual, _, _) = store
        .register_file(
            &p,
            "Manual.pdf",
            location("vol-robin", "doc-1", "/Volumes/Robin/Manual.pdf"),
            Some(hash("manual")),
            audience(&["adults"]),
        )
        .unwrap();
    let (relationship, _) = store.link(&p, "DOCUMENTS", &manual.id, &shared.id, vec![]).unwrap();
    // Hidden activity interleaved with the shared thing's history.
    let (secret, _, _) = store
        .register_file(
            &p,
            "Tax return.pdf",
            location("vol-robin", "doc-2", "/Volumes/Robin/Tax.pdf"),
            Some(hash("tax")),
            Default::default(),
        )
        .unwrap();
    store.link(&p, "REFERENCES", &secret.id, &shared.id, vec![]).unwrap();
    store.link(&p, "RELATED_TO", &manual.id, &shared.id, vec![]).unwrap();

    assert!(matches!(store.verify_chain_as(&robin), Err(Error::Unauthorized(_))));
    store.verify_chain_as(&p).unwrap();

    let owner_view = store.history(&p, &shared.id).unwrap();
    let robin_view = store.history(&robin, &shared.id).unwrap();
    assert!(robin_view.len() < owner_view.len());
    let positions: Vec<i64> = robin_view.iter().map(|e| e.seq).collect();
    assert_eq!(positions, (1..=robin_view.len() as i64).collect::<Vec<_>>(), "no gaps");
    assert!(robin_view.iter().all(|e| e.envelope.body.previous.is_none()));
    assert!(owner_view.iter().any(|e| e.envelope.body.previous.is_some()));
    let owner_seqs: Vec<i64> = owner_view.iter().map(|e| e.seq).collect();
    assert!(owner_seqs.windows(2).any(|w| w[1] - w[0] > 1), "the owner's view keeps the real positions");

    let why = store.why(&robin, &relationship.id).unwrap();
    let positions: Vec<i64> = why.events.iter().map(|e| e.seq).collect();
    assert_eq!(positions, (1..=why.events.len() as i64).collect::<Vec<_>>());
    assert!(why.events.iter().all(|e| e.envelope.body.previous.is_none()));
    let owner_why = store.why(&p, &relationship.id).unwrap();
    assert_eq!(owner_why.events.len(), why.events.len());
    assert!(owner_why.events.iter().any(|e| e.envelope.body.previous.is_some()));

    // Export carries no chain position for anyone.
    let export = store.export(&robin, "k", &SigningKey::from_bytes(&[2; 32])).unwrap();
    assert!(export.events.iter().all(|e| e.envelope.body.previous.is_none()));
    let text = serde_json::to_string(&export).unwrap();
    assert!(!text.contains("\"seq\""));
}

// ------------------------------------------------------------------ #7

#[test]
fn open_does_not_create_a_file_and_a_wrong_owner_learns_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing.sqlite");
    assert!(matches!(Store::open(&missing), Err(Error::NotFound(_))));
    assert!(!missing.exists(), "open must not leave an empty library behind");
    let path = dir.path().join("library.sqlite");
    Store::open_or_create(&path, OWNER).unwrap();
    let error = Store::open_or_create(&path, "hale").unwrap_err();
    assert!(matches!(error, Error::Unauthorized(_)));
    assert!(!error.to_string().contains(OWNER), "{error}");
    assert!(error.to_string().contains("hale"));
}

// ------------------------------------------------------------------ #4

#[test]
fn hashed_integers_beyond_2_to_the_53_are_refused_before_they_enter_the_log() {
    let mut store = fixed_store();
    let p = owner();
    let (_, container, _) = store
        .register_file(
            &p,
            "papers.zip",
            location("vol-robin", "doc-zip", "/Volumes/Robin/papers.zip"),
            Some(hash("zip")),
            Default::default(),
        )
        .unwrap();
    let member = || MemberSpec {
        path: "a.txt".into(),
        size: 1,
        hash: hash("a"),
    };
    let before = store.events(&p).unwrap().len();
    let error = store
        .mount_member(
            &p,
            &container.id,
            member(),
            MountLocator {
                volume_entity: "vol".into(),
                item_id: (1 << 53) + 1,
                mount_epoch: 1,
            },
        )
        .unwrap_err();
    assert!(matches!(
        error,
        Error::Envelope(sc_envelope::ValidationError::IntegerBeyondDouble { .. })
    ), "{error}");
    assert_eq!(store.events(&p).unwrap().len(), before, "nothing was written");
    store
        .mount_member(
            &p,
            &container.id,
            member(),
            MountLocator {
                volume_entity: "vol".into(),
                item_id: 1 << 53,
                mount_epoch: 1,
            },
        )
        .unwrap();
    store.verify_chain().unwrap();
}
