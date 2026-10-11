//! Reflective Rust conformance vector (decision D1) and the `sc-event-0.1`
//! vectors under `fixtures/vectors`.

use ed25519_dalek::{SigningKey, VerifyingKey};
use sc_envelope::{
    AccessLevel, AuthenticatedEnvelope, Body, Envelope, NoteBody, Provenance, ProvenanceKind,
    RR_SCHEMA_VERSION,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const EXPECTED_RR_HASH: &str = "sha256:7c98ac356aa59dd29e466c6f6e5927eb5afd2ad85f283d59d920de3f7bf05a18";

/// The Reflective Rust `type` descriptor body, shaped exactly as the
/// `reflective-rust-protocol` crate serializes it: a tagged enum with
/// `descriptor_kind`/`descriptor`, `layout` omitted when absent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "descriptor_kind", content = "descriptor", rename_all = "kebab-case", deny_unknown_fields)]
enum RrBody {
    Type(RrType),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RrType {
    name: String,
    kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    layout: Option<serde_json::Value>,
    #[serde(default)]
    fields: Vec<serde_json::Value>,
    #[serde(default)]
    variants: Vec<serde_json::Value>,
}

impl Body for RrBody {}

fn rr_vector() -> Envelope<RrBody> {
    Envelope::with_protocol(
        RR_SCHEMA_VERSION,
        "rust:type:Empty",
        "type-1",
        Provenance::new(
            ProvenanceKind::Derived,
            "reflective-rust-conformance",
            "vector:empty-type",
            "sha256:503e43da59434586d85f21cb89b94362a54f394a6877f67fdfc60f715a07e924",
        ),
        AccessLevel::Public,
        RrBody::Type(RrType {
            name: "Empty".into(),
            kind: "struct".into(),
            layout: None,
            fields: vec![],
            variants: vec![],
        }),
    )
}

#[test]
fn reflective_rust_golden_vector_hash_matches() {
    let envelope = rr_vector();
    let canonical = envelope.canonical_json().unwrap();
    assert_eq!(
        canonical,
        r#"{"body":{"descriptor":{"fields":[],"kind":"struct","name":"Empty","variants":[]},"descriptor_kind":"type"},"compatibility":{"protocol":"rr-rsp-0.1","subject_schema":"type-1"},"minimum_access":"public","provenance":{"authority":"reflective-rust-conformance","kind":"derived","source":"vector:empty-type","source_hash":"sha256:503e43da59434586d85f21cb89b94362a54f394a6877f67fdfc60f715a07e924"},"schema_version":"rr-rsp-0.1","subject_id":"rust:type:Empty"}"#
    );
    assert_eq!(envelope.canonical_hash().unwrap(), EXPECTED_RR_HASH);
}

#[test]
fn reflective_rust_vector_with_explicit_null_layout_is_a_different_serialization() {
    // The brief transcribes the body with `"layout":null`. The reference crate
    // skips an absent layout, and only that serialization reaches the recorded
    // hash. This test records the difference instead of hiding it.
    let with_null = serde_json::json!({
        "body": {"descriptor": {"fields": [], "kind": "struct", "layout": null, "name": "Empty", "variants": []}, "descriptor_kind": "type"},
        "compatibility": {"protocol": "rr-rsp-0.1", "subject_schema": "type-1"},
        "minimum_access": "public",
        "provenance": {"authority": "reflective-rust-conformance", "kind": "derived", "source": "vector:empty-type", "source_hash": "sha256:503e43da59434586d85f21cb89b94362a54f394a6877f67fdfc60f715a07e924"},
        "schema_version": "rr-rsp-0.1",
        "subject_id": "rust:type:Empty"
    });
    let bytes = serde_jcs::to_vec(&with_null).unwrap();
    assert_ne!(sc_envelope::hash_bytes(&bytes), EXPECTED_RR_HASH);
    // Parsing that form through the typed body drops the null and lands on the vector.
    let parsed: Envelope<RrBody> = serde_json::from_value(with_null).unwrap();
    assert_eq!(parsed.canonical_hash().unwrap(), EXPECTED_RR_HASH);
}

#[test]
fn reflective_rust_vector_file_matches() {
    let file: VectorFile<RrBody> = read_vector("rr-rsp-0.1-empty-type.json");
    assert_eq!(file.envelope.canonical_json().unwrap(), file.canonical_json);
    assert_eq!(file.envelope.canonical_hash().unwrap(), file.canonical_hash);
    assert_eq!(file.canonical_hash, EXPECTED_RR_HASH);
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct VectorFile<B> {
    #[allow(dead_code)]
    description: String,
    envelope: Envelope<B>,
    canonical_json: String,
    canonical_hash: String,
    #[serde(default)]
    signature: Option<SignatureVector>,
    #[serde(default)]
    expect: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SignatureVector {
    key_id: String,
    /// Test-only seed, published so the vector can be regenerated.
    seed_hex: String,
    verifying_key_hex: String,
    signature: String,
}

fn vectors_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/vectors")
}

fn read_vector<B: Body>(name: &str) -> VectorFile<B> {
    let path = vectors_dir().join(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap()).collect()
}

fn check_sc_vector(name: &str) -> VectorFile<NoteBody> {
    let file: VectorFile<NoteBody> = read_vector(name);
    file.envelope.validate().unwrap();
    assert_eq!(file.envelope.canonical_json().unwrap(), file.canonical_json, "{name}");
    assert_eq!(file.envelope.canonical_hash().unwrap(), file.canonical_hash, "{name}");
    if let Some(sig) = &file.signature {
        let seed: [u8; 32] = unhex(&sig.seed_hex).try_into().unwrap();
        let signing = SigningKey::from_bytes(&seed);
        assert_eq!(hex(signing.verifying_key().as_bytes()), sig.verifying_key_hex, "{name}");
        let verifying = VerifyingKey::from_bytes(&unhex(&sig.verifying_key_hex).try_into().unwrap()).unwrap();
        let authenticated = AuthenticatedEnvelope {
            envelope: file.envelope.clone(),
            authentication: sc_envelope::EnvelopeAuthentication {
                algorithm: sc_envelope::SignatureAlgorithm::Ed25519,
                key_id: sig.key_id.clone(),
                signature: sig.signature.clone(),
            },
        };
        match file.expect.as_deref() {
            None | Some("verifies") => {
                let signed = file.envelope.sign(&sig.key_id, &signing).unwrap();
                assert_eq!(signed.authentication.signature, sig.signature, "{name}: signature bytes");
                authenticated.verify(&sig.key_id, &verifying).unwrap()
            }
            Some("invalid-signature") => assert!(
                matches!(
                    authenticated.verify(&sig.key_id, &verifying),
                    Err(sc_envelope::AuthenticationError::InvalidSignature)
                ),
                "{name}"
            ),
            Some(other) => panic!("{name}: unknown expectation {other}"),
        }
    }
    file
}

#[test]
fn sc_event_declared_vector() {
    check_sc_vector("sc-event-0.1-declared.json");
}

#[test]
fn sc_event_observed_vector() {
    let file = check_sc_vector("sc-event-0.1-observed.json");
    assert_eq!(file.envelope.provenance.kind, ProvenanceKind::Observed);
}

#[test]
fn sc_event_derived_signed_vector() {
    let file = check_sc_vector("sc-event-0.1-derived-signed.json");
    assert!(file.signature.is_some());
}

#[test]
fn sc_event_tampered_signature_vector() {
    let file = check_sc_vector("sc-event-0.1-tampered.json");
    assert_eq!(file.expect.as_deref(), Some("invalid-signature"));
}

#[test]
fn sc_event_rotated_key_vector() {
    // Same envelope, a second key: the identity (canonical hash) does not move
    // with the key, and the old signature does not verify under the new key.
    let first: VectorFile<NoteBody> = read_vector("sc-event-0.1-derived-signed.json");
    let second: VectorFile<NoteBody> = read_vector("sc-event-0.1-rotated-key.json");
    assert_eq!(first.canonical_hash, second.canonical_hash);
    assert_eq!(first.envelope, second.envelope);
    let first_sig = first.signature.unwrap();
    let second_sig = second.signature.unwrap();
    assert_ne!(first_sig.key_id, second_sig.key_id);
    let second_key = VerifyingKey::from_bytes(&unhex(&second_sig.verifying_key_hex).try_into().unwrap()).unwrap();
    let stale = AuthenticatedEnvelope {
        envelope: first.envelope.clone(),
        authentication: sc_envelope::EnvelopeAuthentication {
            algorithm: sc_envelope::SignatureAlgorithm::Ed25519,
            key_id: second_sig.key_id.clone(),
            signature: first_sig.signature.clone(),
        },
    };
    assert!(matches!(
        stale.verify(&second_sig.key_id, &second_key),
        Err(sc_envelope::AuthenticationError::InvalidSignature)
    ));
    check_sc_vector("sc-event-0.1-rotated-key.json");
}

#[test]
fn sc_event_vectors_reject_unknown_fields() {
    let path = vectors_dir().join("sc-event-0.1-declared.json");
    let mut value: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    value["envelope"]["provenance"]["note"] = serde_json::json!("extra");
    assert!(serde_json::from_value::<VectorFile<NoteBody>>(value).is_err());
}

/// Regenerates the `sc-event-0.1` vector files when `SC_WRITE_VECTORS=1`.
/// Otherwise this test only checks the generator agrees with the files.
#[test]
fn sc_event_vectors_are_reproducible() {
    #[derive(Serialize)]
    struct Out<'a> {
        description: &'a str,
        envelope: &'a Envelope<NoteBody>,
        canonical_json: String,
        canonical_hash: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        signature: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        expect: Option<&'a str>,
    }
    let note = |text: &str| NoteBody {
        text: text.into(),
        members: vec![
            sc_envelope::NoteMember { name: "title".into(), minimum_access: AccessLevel::Public },
            sc_envelope::NoteMember { name: "serial".into(), minimum_access: AccessLevel::Owner },
        ],
    };
    let declared = Envelope::new(
        "sc:thing:01J9ZX0000000000000000AVA1",
        "note-1",
        Provenance::new(ProvenanceKind::Declared, "owner-local", "actor:ava", sc_envelope::hash_bytes(b"actor:ava")),
        AccessLevel::Owner,
        note("Ava's dishwasher"),
    )
    .unwrap();
    let observed = Envelope::new(
        "sc:instance:01J9ZX0000000000000000LEO1",
        "note-1",
        Provenance::observed(
            "scanner:leo",
            "file:/Volumes/Robin/Manuals/dishwasher.pdf",
            sc_envelope::hash_bytes(b"dishwasher.pdf"),
            "2026-10-10T12:00:00Z",
            "2026-10-10T12:30:00Z",
        ),
        AccessLevel::Member,
        note("seen at /Volumes/Robin/Manuals/dishwasher.pdf"),
    )
    .unwrap();
    let derived = Envelope::new(
        "sc:relationship:01J9ZX0000000000000000HALE",
        "note-1",
        Provenance::new(ProvenanceKind::Derived, "owner-local", "resolver:within-volume-key", sc_envelope::hash_bytes(b"resolver")),
        AccessLevel::Member,
        note("warranty.pdf DOCUMENTS dishwasher"),
    )
    .unwrap();
    let seed_a = [7_u8; 32];
    let seed_b = [8_u8; 32];
    let key_a = SigningKey::from_bytes(&seed_a);
    let key_b = SigningKey::from_bytes(&seed_b);
    let sig_a = derived.sign("owner-key:jordan:2026-10", &key_a).unwrap().authentication.signature;
    let sig_b = derived.sign("owner-key:jordan:2026-11", &key_b).unwrap().authentication.signature;
    let mut tampered = sig_a.clone();
    let last = tampered.pop().unwrap();
    tampered.push(if last == 'A' { 'B' } else { 'A' });
    let sig_json = |key_id: &str, seed: &[u8; 32], key: &SigningKey, signature: &str| {
        serde_json::json!({
            "key_id": key_id,
            "seed_hex": hex(seed),
            "verifying_key_hex": hex(key.verifying_key().as_bytes()),
            "signature": signature,
        })
    };
    let files = [
        ("sc-event-0.1-declared.json", Out { description: "declared provenance, owner-only, no freshness window", envelope: &declared, canonical_json: declared.canonical_json().unwrap(), canonical_hash: declared.canonical_hash().unwrap(), signature: None, expect: None }),
        ("sc-event-0.1-observed.json", Out { description: "observed provenance with observed_at and expires_at; member access", envelope: &observed, canonical_json: observed.canonical_json().unwrap(), canonical_hash: observed.canonical_hash().unwrap(), signature: None, expect: None }),
        ("sc-event-0.1-derived-signed.json", Out { description: "derived provenance signed with a test seed; domain semantic-core:sc-event:ed25519:v1", envelope: &derived, canonical_json: derived.canonical_json().unwrap(), canonical_hash: derived.canonical_hash().unwrap(), signature: Some(sig_json("owner-key:jordan:2026-10", &seed_a, &key_a, &sig_a)), expect: Some("verifies") }),
        ("sc-event-0.1-tampered.json", Out { description: "the derived vector with its last signature character altered", envelope: &derived, canonical_json: derived.canonical_json().unwrap(), canonical_hash: derived.canonical_hash().unwrap(), signature: Some(sig_json("owner-key:jordan:2026-10", &seed_a, &key_a, &tampered)), expect: Some("invalid-signature") }),
        ("sc-event-0.1-rotated-key.json", Out { description: "the derived vector re-signed under a rotated key; the canonical hash does not change", envelope: &derived, canonical_json: derived.canonical_json().unwrap(), canonical_hash: derived.canonical_hash().unwrap(), signature: Some(sig_json("owner-key:jordan:2026-11", &seed_b, &key_b, &sig_b)), expect: Some("verifies") }),
    ];
    let write = std::env::var_os("SC_WRITE_VECTORS").is_some();
    for (name, out) in files {
        let text = serde_json::to_string_pretty(&out).unwrap() + "\n";
        let path = vectors_dir().join(name);
        if write {
            std::fs::write(&path, &text).unwrap();
        } else {
            let on_disk = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            assert_eq!(on_disk, text, "{name} differs from the generator; run with SC_WRITE_VECTORS=1");
        }
    }
}
