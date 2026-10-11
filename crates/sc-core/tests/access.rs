//! Access fixtures (`fixtures/access/scenario.json`): restricted direct edge,
//! restricted transitive edge, restricted count, restricted search hit,
//! restricted export; a withheld member never reveals its existence.

mod common;

use common::*;
use ed25519_dalek::SigningKey;
use sc_core::*;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AccessScenario {
    name: String,
    owner: String,
    principals: BTreeMap<String, Vec<String>>,
    things: Vec<ThingStep>,
    files: Vec<AccessFile>,
    links: Vec<LinkStep>,
    expectations: Vec<Expectation>,
    transitive: Vec<Transitive>,
    searches: Vec<Search>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AccessFile {
    #[serde(rename = "as")]
    name: String,
    label: String,
    path: String,
    audience: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Expectation {
    principal: String,
    entity: String,
    #[serde(default)]
    count: Option<usize>,
    #[serde(default)]
    labels: Option<Vec<String>>,
    #[serde(default)]
    not_found: bool,
    #[serde(default)]
    visible: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Transitive {
    principal: String,
    start: String,
    depth: usize,
    reaches: Vec<String>,
    #[serde(default)]
    never: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Search {
    principal: String,
    text: String,
    hits: Vec<String>,
}

struct Built {
    store: Store,
    names: BTreeMap<String, String>,
    principals: BTreeMap<String, Principal>,
    scenario: AccessScenario,
}

impl Built {
    fn id(&self, name: &str) -> String {
        self.names[name].clone()
    }
    fn principal(&self, name: &str) -> &Principal {
        &self.principals[name]
    }
}

fn build() -> Built {
    let text = std::fs::read_to_string(fixtures_dir().join("access/scenario.json")).unwrap();
    let scenario: AccessScenario = serde_json::from_str(&text).unwrap();
    let mut store = Store::in_memory(&scenario.owner).unwrap();
    let p = Principal::solo(&scenario.owner);
    let mut names = BTreeMap::new();
    for thing in &scenario.things {
        let created = store
            .create_thing(&p, &thing.label, thing.audience.iter().cloned().collect())
            .unwrap();
        names.insert(thing.name.clone(), created.id);
    }
    for (index, file) in scenario.files.iter().enumerate() {
        let (artifact, _, instance) = store
            .register_file(
                &p,
                &file.label,
                location("vol-robin", &format!("doc-{index}"), &file.path),
                Some(hash(&file.name)),
                file.audience.iter().cloned().collect(),
            )
            .unwrap();
        names.insert(format!("{}.artifact", file.name), artifact.id);
        names.insert(file.name.clone(), instance.id);
    }
    for link in &scenario.links {
        let (relationship, _) = store
            .link(&p, &link.predicate, &names[&link.source], &names[&link.target], vec![])
            .unwrap();
        names.insert(link.name.clone(), relationship.id);
    }
    let principals = scenario
        .principals
        .iter()
        .map(|(name, audiences)| (name.clone(), Principal::new(name, audiences.iter().cloned())))
        .collect();
    Built {
        store,
        names,
        principals,
        scenario,
    }
}

#[test]
fn restricted_direct_edges_and_counts() {
    let built = build();
    for expectation in &built.scenario.expectations {
        let p = built.principal(&expectation.principal);
        let id = built.id(&expectation.entity);
        let context = format!("{}: {} looks at {}", built.scenario.name, expectation.principal, expectation.entity);
        if expectation.not_found {
            // Withheld reads the same as absent, for every accessor.
            assert!(matches!(built.store.kind_of(p, &id), Err(Error::NotFound(_))), "{context}");
            assert!(matches!(built.store.inspect(p, &id), Err(Error::NotFound(_))), "{context}");
            assert!(matches!(built.store.relationships_of(p, &id), Err(Error::NotFound(_))), "{context}");
            assert!(matches!(built.store.count_relationships(p, &id), Err(Error::NotFound(_))), "{context}");
            assert!(matches!(built.store.history(p, &id), Err(Error::NotFound(_))), "{context}");
            let absent = built.store.kind_of(p, "01J00000000000000000000000").unwrap_err().to_string();
            let withheld = built.store.kind_of(p, &id).unwrap_err().to_string();
            assert_eq!(absent.replace("01J00000000000000000000000", ""), withheld.replace(&id, ""), "{context}");
            continue;
        }
        if expectation.visible {
            built.store.inspect(p, &id).expect(&context);
        }
        if let Some(count) = expectation.count {
            assert_eq!(built.store.count_relationships(p, &id).expect(&context), count, "{context}");
        }
        if let Some(labels) = &expectation.labels {
            let mut actual: Vec<String> = built
                .store
                .relationships_of(p, &id)
                .expect(&context)
                .into_iter()
                .map(|v| v.label)
                .collect();
            actual.sort();
            let mut expected = labels.clone();
            expected.sort();
            assert_eq!(actual, expected, "{context}");
        }
    }
}

#[test]
fn restricted_transitive_edges() {
    let built = build();
    for case in &built.scenario.transitive {
        let p = built.principal(&case.principal);
        let reached = built.store.reachable(p, &built.id(&case.start), case.depth).unwrap();
        let expected: BTreeSet<String> = case.reaches.iter().map(|n| built.id(n)).collect();
        assert_eq!(reached, expected, "{} from {}", case.principal, case.start);
        for never in &case.never {
            assert!(!reached.contains(&built.id(never)), "{} must not reach {never}", case.principal);
        }
    }
}

#[test]
fn restricted_search_hits() {
    let built = build();
    for search in &built.scenario.searches {
        let p = built.principal(&search.principal);
        let hits: BTreeSet<String> = built
            .store
            .search(p, &search.text)
            .unwrap()
            .into_iter()
            .map(|h| h.id)
            .collect();
        let expected: BTreeSet<String> = search.hits.iter().map(|n| built.id(n)).collect();
        assert_eq!(hits, expected, "{} searches {:?}", search.principal, search.text);
    }
}

#[test]
fn restricted_export_and_history() {
    let built = build();
    let key = SigningKey::from_bytes(&[9_u8; 32]);
    let owner_export = built.store.export(built.principal("ava"), "owner-key", &key).unwrap();
    let robin_export = built.store.export(built.principal("robin"), "owner-key", &key).unwrap();
    let leo_export = built.store.export(built.principal("leo"), "owner-key", &key).unwrap();
    let hale_export = built.store.export(built.principal("hale"), "owner-key", &key).unwrap();
    assert!(owner_export.events.len() > robin_export.events.len());
    assert!(robin_export.events.len() > leo_export.events.len());
    assert!(hale_export.events.is_empty());

    let mentions = |export: &Interchange, id: &str| {
        serde_json::to_string(export).unwrap().contains(id)
    };
    let safe = built.id("safe");
    let combination = built.id("combination.artifact");
    let tax = built.id("tax.artifact");
    assert!(mentions(&owner_export, &safe));
    for export in [&robin_export, &leo_export, &hale_export] {
        assert!(!mentions(export, &safe), "safe leaks");
        assert!(!mentions(export, &combination), "combination leaks");
        assert!(!mentions(export, &built.id("tax-references-safe")), "restricted edge leaks");
    }
    assert!(mentions(&robin_export, &tax));
    assert!(!mentions(&leo_export, &tax));
    // Signed with the owner's key, and the export is self-consistent.
    for signed in &robin_export.events {
        signed.verify("owner-key", &key.verifying_key()).unwrap();
    }

    // History of a shared thing hides the events about its restricted edge.
    let dishwasher = built.id("dishwasher");
    let leo_history = built.store.history(built.principal("leo"), &dishwasher).unwrap();
    assert!(!leo_history.iter().any(|e| serde_json::to_string(e).unwrap().contains(&tax)));
    let robin_history = built.store.history(built.principal("robin"), &dishwasher).unwrap();
    assert!(robin_history.iter().any(|e| serde_json::to_string(e).unwrap().contains(&tax)));
}

#[test]
fn derived_objects_inherit_the_most_private_audience() {
    let built = build();
    let receipt = built.id("receipt-documents");
    let relationship = built.store.get_relationship(built.principal("ava"), &receipt).unwrap();
    assert_eq!(relationship.access.audience, audience(&["kids"]));
    // Robin sees the receipt but not the bike, so not the edge either.
    assert!(built.store.inspect(built.principal("robin"), &built.id("bike-receipt.artifact")).is_ok());
    assert!(matches!(
        built.store.get_relationship(built.principal("robin"), &receipt),
        Err(Error::NotFound(_))
    ));
    assert_eq!(
        built
            .store
            .count_relationships(built.principal("robin"), &built.id("bike-receipt.artifact"))
            .unwrap(),
        0
    );
}
