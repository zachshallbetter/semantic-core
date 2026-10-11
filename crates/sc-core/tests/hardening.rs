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
                window: None,
            },
        )
        .unwrap();
    assert_eq!(ambiguous.outcome, Outcome::Ambiguous);

    // Before narrowing, robin sees everything under the artifact, directly or in collections.
    store.get_resolution(&robin, &ambiguous.id).unwrap();
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
    // Direct reads follow the same rule as the collection.
    assert!(matches!(store.get_resolution(&robin, &ambiguous.id), Err(Error::NotFound(_))));
    store.get_resolution(&p, &ambiguous.id).unwrap();
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
            // The file claims the flag; the importer derives it from the predicate.
            relationship.materializes = true;
            *signed = signed.envelope.sign("k", &signing).unwrap();
        }
    }
    let mut target = fixed_store();
    target
        .import(&p, &export, &TrustedKeys::new().trust(OWNER, "k", signing.verifying_key()), None)
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

#[test]
fn resolutions_follow_one_rule_directly_and_in_collections() {
    let mut store = fixed_store();
    let p = owner();
    let robin = Principal::new("robin", ["adults"]);
    let (_, _, shared) = store
        .register_file(
            &p,
            "Manual.pdf",
            location("vol-robin", "doc-1", "/Volumes/Robin/Manual.pdf"),
            Some(hash("manual")),
            audience(&["adults"]),
        )
        .unwrap();
    let (_, _, hidden) = store
        .register_file(
            &p,
            "Tax.pdf",
            location("vol-robin", "doc-2", "/Volumes/Robin/Tax.pdf"),
            Some(hash("tax")),
            Default::default(),
        )
        .unwrap();
    // RESOLVED, naming a visible instance: readable both ways.
    let seen_shared = store
        .observe(&p, Seen { location: location("vol-robin", "doc-1", "/Volumes/Robin/Kitchen/Manual.pdf"), content_hash: None, source: "scan".into(), window: None })
        .unwrap();
    assert_eq!(seen_shared.outcome, Outcome::Resolved);
    store.get_resolution(&robin, &seen_shared.id).unwrap();
    assert!(store.resolutions_for_instance(&robin, &shared.id).unwrap().iter().any(|r| r.id == seen_shared.id));
    // RESOLVED, naming a hidden instance: withheld both ways.
    let seen_hidden = store
        .observe(&p, Seen { location: location("vol-robin", "doc-2", "/Volumes/Robin/Private/Tax.pdf"), content_hash: None, source: "scan".into(), window: None })
        .unwrap();
    assert!(matches!(store.get_resolution(&robin, &seen_hidden.id), Err(Error::NotFound(_))));
    assert!(matches!(store.resolutions_for_instance(&robin, &hidden.id), Err(Error::NotFound(_))));
    store.get_resolution(&p, &seen_hidden.id).unwrap();
    // UNRESOLVED, naming nothing: the owner's alone.
    let nothing = store
        .observe(&p, Seen { location: location("vol-leo", "doc-9", "/Volumes/Leo/x.pdf"), content_hash: Some(hash("x")), source: "scan".into(), window: None })
        .unwrap();
    assert_eq!(nothing.outcome, Outcome::Unresolved);
    assert!(matches!(store.get_resolution(&robin, &nothing.id), Err(Error::NotFound(_))));
    store.get_resolution(&p, &nothing.id).unwrap();
}

// ------------------------------------------------------------------ #6, recomputation

/// A library where hidden events sit between a member's visible ones,
/// including a retraction.
fn interleaved() -> (Store, String, String) {
    let mut store = fixed_store();
    let p = owner();
    let shared = store.create_thing(&p, "Dishwasher", audience(&["adults"])).unwrap();
    let (manual, _, _) = store
        .register_file(&p, "Manual.pdf", location("vol-robin", "doc-1", "/Volumes/Robin/Manual.pdf"), Some(hash("manual")), audience(&["adults"]))
        .unwrap();
    let (relationship, assertion) = store.link(&p, "DOCUMENTS", &manual.id, &shared.id, vec![]).unwrap();
    let (secret, _, _) = store
        .register_file(&p, "Tax.pdf", location("vol-robin", "doc-2", "/Volumes/Robin/Tax.pdf"), Some(hash("tax")), Default::default())
        .unwrap();
    store.link(&p, "REFERENCES", &secret.id, &shared.id, vec![]).unwrap();
    store.retract_assertion(&p, &assertion.id, "wrong appliance").unwrap();
    store.link(&p, "RELATED_TO", &manual.id, &shared.id, vec![]).unwrap();
    (store, shared.id, relationship.id)
}

#[test]
fn a_restricted_viewer_cannot_recompute_hashes_to_detect_withheld_events() {
    let (store, shared, relationship) = interleaved();
    let p = owner();
    let robin = Principal::new("robin", ["adults"]);
    let leo = Principal::new("leo", ["adults"]);
    let stored: Vec<String> = store.events(&p).unwrap().into_iter().map(|e| e.hash).collect();

    let view = store.history(&robin, &shared).unwrap();
    let why = store.why(&robin, &relationship).unwrap();
    // The attack: take each shown envelope, try every plausible predecessor
    // (the previous shown event, or any stored hash at all), recompute, and
    // compare with the identifier shown. Nothing matches.
    for window in view.windows(2) {
        let mut guess = window[1].envelope.clone();
        let mut candidates = vec![window[0].hash.clone()];
        candidates.extend(stored.iter().cloned());
        for candidate in candidates {
            guess.body.previous = Some(candidate);
            assert_ne!(guess.canonical_hash().unwrap(), window[1].hash);
        }
    }
    // No stored hash appears anywhere in what the viewer receives,
    // including the retraction's `compensates`.
    let shown = serde_json::to_string(&view).unwrap() + &serde_json::to_string(&why).unwrap();
    for hash in &stored {
        assert!(!shown.contains(hash), "stored hash leaked to a restricted viewer");
    }
    assert!(view.iter().all(|e| e.hash.starts_with("view:")));
    let mut saw_retraction = false;
    for event in &why.events {
        if let Change::AssertionRetracted { compensates, .. } = &event.envelope.body.change {
            saw_retraction = true;
            assert!(compensates.starts_with("view:"));
            assert!(why.events.iter().any(|e| &e.hash == compensates), "compensates still resolves inside the viewer's own list");
        }
    }
    assert!(saw_retraction);
    // Identifiers are stable for one viewer and differ between viewers.
    let again = store.history(&robin, &shared).unwrap();
    assert_eq!(view.iter().map(|e| &e.hash).collect::<Vec<_>>(), again.iter().map(|e| &e.hash).collect::<Vec<_>>());
    let leo_view = store.history(&leo, &shared).unwrap();
    assert_eq!(leo_view.len(), view.len());
    assert!(leo_view.iter().zip(&view).all(|(a, b)| a.hash != b.hash));
    // The owner still sees the stored hashes.
    assert!(store.history(&p, &shared).unwrap().iter().all(|e| e.hash.starts_with("sha256:")));
}

#[test]
fn a_restricted_export_carries_no_stored_hash_and_still_round_trips() {
    let (mut store, _, _) = interleaved();
    let p = owner();
    let robin = Principal::new("robin", ["adults"]);
    let stored: Vec<String> = store.events(&p).unwrap().into_iter().map(|e| e.hash).collect();
    let signing = SigningKey::from_bytes(&[2; 32]);
    let keys = || TrustedKeys::new().trust(OWNER, "k", signing.verifying_key());
    let export = store.export(&robin, "k", &signing).unwrap();
    let text = serde_json::to_string(&export).unwrap();
    for hash in &stored {
        assert!(!text.contains(hash), "stored hash left in a restricted export");
    }
    assert!(export.events.iter().all(|e| e.envelope.body.origin.as_deref().is_some_and(|o| o.starts_with("origin:"))));
    assert!(export.events.iter().any(|e| matches!(&e.envelope.body.change, Change::AssertionRetracted { compensates, .. } if compensates.starts_with("origin:"))));
    // The same oracle does not exist here either: guessing a predecessor and
    // recomputing never matches the alias.
    for signed in &export.events {
        let mut guess = signed.envelope.clone();
        for candidate in &stored {
            guess.body.previous = Some(candidate.clone());
            assert_ne!(Some(guess.canonical_hash().unwrap()), signed.envelope.body.origin);
        }
    }
    // Coming back to the library it left, every event is recognised.
    let report = store.import(&p, &export, &keys(), None).unwrap();
    assert_eq!(report.applied, 0, "{report:?}");
    // And it imports into another library once.
    let mut other = fixed_store();
    assert!(other.import(&p, &export, &keys(), None).unwrap().applied > 0);
    assert_eq!(other.import(&p, &export, &keys(), None).unwrap().applied, 0);
    // The owner's own export names stored hashes, for the owner.
    let owner_export = store.export(&p, "k", &signing).unwrap();
    assert!(owner_export.events.iter().any(|e| e.envelope.body.origin.as_deref().is_some_and(|o| o.starts_with("sha256:"))));
}

#[test]
fn a_pre_forked_library_fails_to_open_with_a_readable_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.sqlite");
    {
        let mut store = Store::open_or_create(&path, OWNER).unwrap();
        store.create_thing(&owner(), "Dishwasher", Default::default()).unwrap();
    }
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch("DROP INDEX events_previous;").unwrap();
    let (previous, envelope): (Option<String>, String) = conn
        .query_row("SELECT previous, envelope FROM events ORDER BY seq DESC LIMIT 1", [], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap();
    conn.execute(
        "INSERT INTO events (hash, previous, origin, subject, at, envelope) VALUES ('sha256:fork', ?1, NULL, 's', 't', ?2)",
        rusqlite::params![previous, envelope],
    )
    .unwrap();
    drop(conn);
    let error = Store::open(&path).unwrap_err();
    assert!(matches!(error, Error::Integrity(_)), "{error}");
    assert!(error.to_string().contains("fork"), "{error}");
}
