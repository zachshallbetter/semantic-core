//! Decisions recorded on the open questions: freshness binds at observe time
//! and on replay; a relationship's history folds in its assertions; import
//! fails closed on a foreign owner; imported origins are aliased for
//! restricted viewers.

mod common;

use common::*;
use ed25519_dalek::SigningKey;
use sc_core::*;

fn keys(signing: &SigningKey) -> TrustedKeys {
    TrustedKeys::new().trust(OWNER, "k", signing.verifying_key())
}

// ------------------------------------------------------------------ #10

#[test]
fn an_expired_observation_is_recorded_but_drives_nothing() {
    let mut store = fixed_store(); // clock: 2026-10-10T12:00:00Z
    let p = owner();
    let (_, _, instance) = store
        .register_file(
            &p,
            "Warranty.pdf",
            location("vol-robin", "doc-1", "/Volumes/Robin/Warranty.pdf"),
            Some(hash("warranty")),
            Default::default(),
        )
        .unwrap();
    let before = store.events(&p).unwrap().len();
    let stale = Seen {
        location: location("vol-robin", "doc-1", "/Volumes/Robin/Archive/Warranty.pdf"),
        content_hash: Some(hash("warranty")),
        source: "yesterday's scan".into(),
        window: Some(("2026-10-09T12:00:00Z".into(), "2026-10-09T13:00:00Z".into())),
    };
    let resolution = store.observe(&p, stale).unwrap();
    assert_eq!(resolution.outcome, Outcome::Unresolved);
    assert_eq!(resolution.rule, Rule::NoMatch);
    assert!(resolution.instance_id.is_none());
    let freshness = resolution.evidence.iter().find(|e| e.kind == "freshness").unwrap();
    assert!(freshness.value.starts_with("expired:"), "{freshness:?}");
    // Nothing moved; the observation itself stays in the log.
    let unchanged = store.get_instance(&p, &instance.id).unwrap();
    assert_eq!(unchanged.location.path, "/Volumes/Robin/Warranty.pdf");
    assert_eq!(store.events(&p).unwrap().len(), before + 1);
    store.get_observation(&p, &resolution.observation_id).unwrap();
    store.get_resolution(&p, &resolution.id).unwrap();

    // A window that is still open drives the resolution as usual.
    let fresh = Seen {
        location: location("vol-robin", "doc-1", "/Volumes/Robin/Archive/Warranty.pdf"),
        content_hash: Some(hash("warranty")),
        source: "scan".into(),
        window: Some(("2026-10-10T11:30:00Z".into(), "2026-10-10T12:30:00Z".into())),
    };
    let resolution = store.observe(&p, fresh).unwrap();
    assert_eq!(resolution.outcome, Outcome::Resolved);
    assert_eq!(store.get_instance(&p, &instance.id).unwrap().location.path, "/Volumes/Robin/Archive/Warranty.pdf");

    // Not yet valid is inadmissible too.
    let future = Seen {
        location: location("vol-robin", "doc-1", "/Volumes/Robin/Later/Warranty.pdf"),
        content_hash: Some(hash("warranty")),
        source: "scan".into(),
        window: Some(("2026-10-10T13:00:00Z".into(), "2026-10-10T14:00:00Z".into())),
    };
    let resolution = store.observe(&p, future).unwrap();
    assert_eq!(resolution.outcome, Outcome::Unresolved);
    assert!(resolution.evidence.iter().any(|e| e.kind == "freshness" && e.value.starts_with("not-yet-valid:")));
    store.verify_chain().unwrap();
    store.replay().unwrap();
}

#[test]
fn replay_and_import_refuse_a_resolution_that_rested_on_an_expired_observation() {
    let mut source = fixed_store();
    let p = owner();
    let (_, _, instance) = source
        .register_file(
            &p,
            "Warranty.pdf",
            location("vol-robin", "doc-1", "/Volumes/Robin/Warranty.pdf"),
            Some(hash("warranty")),
            Default::default(),
        )
        .unwrap();
    source
        .observe(
            &p,
            Seen {
                location: location("vol-robin", "doc-1", "/Volumes/Robin/Moved.pdf"),
                content_hash: Some(hash("warranty")),
                source: "scan".into(),
                window: None,
            },
        )
        .unwrap();
    assert_eq!(source.get_instance(&p, &instance.id).unwrap().location.path, "/Volumes/Robin/Moved.pdf");
    let signing = SigningKey::from_bytes(&[2; 32]);
    let honest = source.export(&p, "k", &signing).unwrap();

    // Forge the log a recipient would replay: the same RESOLVED resolution,
    // but the observation's window ended before the event was recorded.
    let mut forged = honest.clone();
    for signed in &mut forged.events {
        if let Change::ObservationRecorded { observation, .. } = &mut signed.envelope.body.change {
            observation.observed_at = "2026-10-10T10:00:00Z".into();
            observation.expires_at = "2026-10-10T11:00:00Z".into();
            signed.envelope.provenance.observed_at = Some("2026-10-10T10:00:00Z".into());
            signed.envelope.provenance.expires_at = Some("2026-10-10T11:00:00Z".into());
            *signed = signed.envelope.sign("k", &signing).unwrap();
        }
    }
    let mut target = fixed_store();
    let error = target.import(&p, &forged, &keys(&signing), None).unwrap_err();
    assert!(matches!(error, Error::Integrity(_)), "{error}");
    assert!(error.to_string().contains("expired"), "{error}");
    assert_eq!(target.events(&p).unwrap().len(), 1, "all or nothing");
    // The honest file imports, and replays, because the window held at the time.
    target.import(&p, &honest, &keys(&signing), None).unwrap();
    target.replay().unwrap();
    assert_eq!(target.get_instance(&p, &instance.id).unwrap().location.path, "/Volumes/Robin/Moved.pdf");
}

// ------------------------------------------------------------------ #12

#[test]
fn relationship_history_includes_every_assertion_in_sequence() {
    let mut store = fixed_store();
    let p = owner();
    let thing = store.create_thing(&p, "Dishwasher", Default::default()).unwrap();
    let (artifact, _, _) = store
        .register_file(
            &p,
            "Warranty.pdf",
            location("vol-robin", "doc-1", "/Volumes/Robin/Warranty.pdf"),
            Some(hash("warranty")),
            Default::default(),
        )
        .unwrap();
    let (relationship, first) = store.link(&p, "DOCUMENTS", &artifact.id, &thing.id, vec![]).unwrap();
    let second = store
        .propose_assertion(&p, &relationship.id, Polarity::Disputes, vec![Evidence::new("note", "wrong model")])
        .unwrap();
    store.record_qualification(&p, &second.id, Qualification::Inconclusive).unwrap();
    store.accept_assertion(&p, &second.id, OWNER_LOCAL).unwrap();
    store.retract_assertion(&p, &first.id, "superseded").unwrap();

    let history = store.history(&p, &relationship.id).unwrap();
    let ops: Vec<String> = history
        .iter()
        .map(|e| match &e.envelope.body.change {
            Change::RelationshipDeclared { .. } => "declared".to_string(),
            Change::AssertionProposed { assertion } => format!("proposed {}", short(&assertion.id, &first.id, &second.id)),
            Change::QualificationRecorded { assertion_id, .. } => format!("qualified {}", short(assertion_id, &first.id, &second.id)),
            Change::AssertionAccepted { assertion_id, .. } => format!("accepted {}", short(assertion_id, &first.id, &second.id)),
            Change::AssertionRetracted { assertion_id, .. } => format!("retracted {}", short(assertion_id, &first.id, &second.id)),
            _ => "other".to_string(),
        })
        .collect();
    assert_eq!(
        ops,
        [
            "declared",
            "proposed first",
            "accepted first",
            "proposed second",
            "qualified second",
            "accepted second",
            "retracted first"
        ]
    );
    assert!(history.windows(2).all(|w| w[0].seq < w[1].seq));
    // `why` is unchanged in substance: the same events, grouped by assertion.
    let why = store.why(&p, &relationship.id).unwrap();
    assert_eq!(why.events.len(), history.len());
    assert_eq!(why.assertions.len(), 2);
    // Other entities keep the narrow history.
    assert!(store
        .history(&p, &thing.id)
        .unwrap()
        .iter()
        .all(|e| !matches!(e.envelope.body.change, Change::AssertionAccepted { .. })));
}

fn short(id: &str, first: &str, second: &str) -> &'static str {
    if id == first {
        "first"
    } else if id == second {
        "second"
    } else {
        "unknown"
    }
}

// ------------------------------------------------------------------ #13 is in tests/interchange.rs
// (different_owners_rebase_onto_the_importer_and_record_the_source_owner).

// ------------------------------------------------------------------ #15

#[test]
fn imported_origins_are_aliased_for_restricted_viewers_and_exports() {
    let mut source = fixed_store();
    let p = owner();
    // Audiences are projected on export, so in the target the thing is for "member".
    let robin = Principal::new("robin", ["member"]);
    let thing = source.create_thing(&p, "Dishwasher", audience(&["adults"])).unwrap();
    let (artifact, _, _) = source
        .register_file(
            &p,
            "Warranty.pdf",
            location("vol-robin", "doc-1", "/Volumes/Robin/Warranty.pdf"),
            Some(hash("warranty")),
            audience(&["adults"]),
        )
        .unwrap();
    source.link(&p, "DOCUMENTS", &artifact.id, &thing.id, vec![]).unwrap();
    let signing = SigningKey::from_bytes(&[2; 32]);
    let export = source.export(&p, "k", &signing).unwrap();
    let source_hashes: Vec<String> = source.events(&p).unwrap().into_iter().map(|e| e.hash).collect();

    // Same owner, second library: every imported event carries a stored
    // hash of the source as its origin.
    let mut target = fixed_store();
    target.import(&p, &export, &keys(&signing), None).unwrap();
    assert!(target
        .events(&p)
        .unwrap()
        .iter()
        .skip(1)
        .all(|e| e.envelope.body.origin.as_deref().is_some_and(|o| o.starts_with("sha256:"))));

    // A restricted viewer of the target sees no sha256 value at all.
    let history = target.history(&robin, &thing.id).unwrap();
    assert!(!history.is_empty());
    // `provenance.source_hash` is a digest of the actor string, not of any
    // log; every other sha256 value is a log hash and must be absent.
    let shown = without_source_hash(&serde_json::to_string(&history).unwrap());
    assert!(!shown.contains("sha256:"), "{shown}");
    for hash in &source_hashes {
        assert!(!shown.contains(hash));
    }
    assert!(history.iter().all(|e| e.envelope.body.origin.as_deref().is_some_and(|o| o.starts_with("view:"))));

    // A restricted export of the target aliases the origins too.
    let restricted = target.export(&robin, "k", &signing).unwrap();
    let text = without_source_hash(&serde_json::to_string(&restricted).unwrap());
    assert!(!text.contains("sha256:"), "{text}");
    assert!(restricted.events.iter().all(|e| e.envelope.body.origin.as_deref().is_some_and(|o| o.starts_with("origin:"))));
    // And it is still recognised when it comes back.
    let report = target.import(&p, &restricted, &keys(&signing), None).unwrap();
    assert_eq!(report.applied, 0, "{report:?}");
    // The owner's export of the target keeps the source's stored origins.
    let owner_export = target.export(&p, "k", &signing).unwrap();
    assert!(owner_export.events.iter().all(|e| e.envelope.body.origin.as_deref().is_some_and(|o| o.starts_with("sha256:"))));
    assert_eq!(source.import(&p, &owner_export, &keys(&signing), None).unwrap().applied, 0);
}

/// Strip the digests that are not log hashes (`source_hash` of an actor
/// string, `content_hash` of bytes, `member_hash` of a member) so the
/// remaining `sha256:` values are all event references.
fn without_source_hash(text: &str) -> String {
    let mut out = text.to_string();
    for field in ["source_hash", "content_hash", "member_hash"] {
        let marker = format!("\"{field}\":\"sha256:");
        let mut acc = String::new();
        let mut rest = out.as_str();
        while let Some(i) = rest.find(&marker) {
            acc.push_str(&rest[..i]);
            let after = &rest[i + marker.len()..];
            let end = after.find('"').unwrap();
            rest = &after[end..];
        }
        acc.push_str(rest);
        out = acc;
    }
    out
}

// ------------------------------------------------------------------ #17

#[test]
fn replay_and_import_refuse_a_resolution_resting_on_a_not_yet_valid_observation() {
    let mut source = fixed_store();
    let p = owner();
    let (_, _, instance) = source
        .register_file(
            &p,
            "Warranty.pdf",
            location("vol-robin", "doc-1", "/Volumes/Robin/Warranty.pdf"),
            Some(hash("warranty")),
            Default::default(),
        )
        .unwrap();
    source
        .observe(
            &p,
            Seen {
                location: location("vol-robin", "doc-1", "/Volumes/Robin/Moved.pdf"),
                content_hash: Some(hash("warranty")),
                source: "scan".into(),
                window: None,
            },
        )
        .unwrap();
    let signing = SigningKey::from_bytes(&[2; 32]);
    let honest = source.export(&p, "k", &signing).unwrap();
    // Forge: the observation's window opens after the event was recorded.
    let mut forged = honest.clone();
    for signed in &mut forged.events {
        if let Change::ObservationRecorded { observation, .. } = &mut signed.envelope.body.change {
            observation.observed_at = "2026-10-10T13:00:00Z".into();
            observation.expires_at = "2026-10-10T14:00:00Z".into();
            signed.envelope.provenance.observed_at = Some("2026-10-10T13:00:00Z".into());
            signed.envelope.provenance.expires_at = Some("2026-10-10T14:00:00Z".into());
            *signed = signed.envelope.sign("k", &signing).unwrap();
        }
    }
    let mut target = fixed_store();
    let error = target.import(&p, &forged, &keys(&signing), None).unwrap_err();
    assert!(matches!(error, Error::Integrity(_)), "{error}");
    assert!(error.to_string().contains("not valid before"), "{error}");
    assert_eq!(target.events(&p).unwrap().len(), 1, "all or nothing");
    target.import(&p, &honest, &keys(&signing), None).unwrap();
    target.replay().unwrap();
    assert_eq!(target.get_instance(&p, &instance.id).unwrap().location.path, "/Volumes/Robin/Moved.pdf");
}

// ------------------------------------------------------------------ #18

#[test]
fn an_origin_alias_travels_unchanged_and_is_recognised_by_the_library_that_minted_it() {
    let p = owner();
    let robin = Principal::new("robin", ["member"]);
    let signing = SigningKey::from_bytes(&[2; 32]);
    let mut a = fixed_store();
    let thing = a.create_thing(&p, "Dishwasher", audience(&["member"])).unwrap();
    let (artifact, _, _) = a
        .register_file(
            &p,
            "Warranty.pdf",
            location("vol-robin", "doc-1", "/Volumes/Robin/Warranty.pdf"),
            Some(hash("warranty")),
            audience(&["member"]),
        )
        .unwrap();
    a.link(&p, "DOCUMENTS", &artifact.id, &thing.id, vec![]).unwrap();

    // A -> B -> C -> A with owner exports: stored origins pass through and A
    // recognises every event when the file comes home.
    let mut b = fixed_store();
    b.import(&p, &a.export(&p, "k", &signing).unwrap(), &keys(&signing), None).unwrap();
    let mut c = fixed_store();
    c.import(&p, &b.export(&p, "k", &signing).unwrap(), &keys(&signing), None).unwrap();
    let home = c.export(&p, "k", &signing).unwrap();
    let report = a.import(&p, &home, &keys(&signing), None).unwrap();
    assert_eq!(report.applied, 0, "{report:?}");

    // A restricted export at B mints B's aliases. They travel through C
    // unchanged (no second aliasing) and B recognises them when they return.
    let from_b = b.export(&robin, "k", &signing).unwrap();
    let minted: Vec<String> = from_b.events.iter().map(|e| e.envelope.body.origin.clone().unwrap()).collect();
    assert!(minted.iter().all(|o| o.starts_with("origin:")));
    let mut c2 = fixed_store();
    c2.import(&p, &from_b, &keys(&signing), None).unwrap();
    let onward = c2.export(&robin, "k", &signing).unwrap();
    let carried: Vec<String> = onward.events.iter().map(|e| e.envelope.body.origin.clone().unwrap()).collect();
    assert_eq!(carried, minted, "an existing alias is carried through, not aliased again");
    let back_to_b = b.import(&p, &onward, &keys(&signing), None).unwrap();
    assert_eq!(back_to_b.applied, 0, "{back_to_b:?}");
    // The owner export of c2 carries the same alias and B recognises that too.
    let owner_onward = c2.export(&p, "k", &signing).unwrap();
    assert_eq!(b.import(&p, &owner_onward, &keys(&signing), None).unwrap().applied, 0);
    // The alias was keyed by B's secret: A, which never saw B's export, is
    // not the library it names, and treats the events as new.
    let mut a2 = fixed_store();
    assert!(a2.import(&p, &onward, &keys(&signing), None).unwrap().applied > 0);
    // Nothing above exposed a stored hash to the restricted viewer.
    let text = without_source_hash(&serde_json::to_string(&onward).unwrap());
    assert!(!text.contains("sha256:"), "{text}");
}
