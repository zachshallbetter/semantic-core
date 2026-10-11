//! `sc-event-0.1` envelope.
//!
//! The envelope copies the Reflective Rust RSP envelope discipline field for
//! field (definition section 5 and decision D1): `schema_version`, `subject_id`,
//! `compatibility {protocol, subject_schema}`, `provenance {kind, authority,
//! source, source_hash, observed_at?, expires_at?}`, `minimum_access`, `body`.
//!
//! Implemented and tested here:
//!
//! - JCS canonical JSON (RFC 8785) and the `sha256:<hex>` identity of the
//!   canonical bytes. The Reflective Rust golden vector (`rr-rsp-0.1`,
//!   subject `rust:type:Empty`) reproduces byte for byte; see `tests/`.
//! - Ed25519 signatures over `domain || key_id || 0x00 || canonical`, encoded
//!   base64url without padding, under Semantic Core's own domain string.
//! - Every struct rejects unknown fields.
//! - Observation rule: `observed` provenance must carry `observed_at` and
//!   `expires_at` with expiry after observation; other kinds must not.
//! - Access projection: `public < member < owner < administrator` as a
//!   disclosure projection, applied to the envelope and delegated to the body.
//! - Freshness: `require_fresh_at` for observed envelopes.
//!
//! The envelope is generic over its body. Bodies are closed types that
//! implement [`Body`]; the crate does not accept free-form JSON bodies because
//! that would defeat the unknown-field rule.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

/// Protocol identifier for Semantic Core events.
pub const SCHEMA_VERSION: &str = "sc-event-0.1";

/// Protocol identifier of the Reflective Rust reference. Accepted only so the
/// golden vector can be reproduced; `validate` pins `sc-event-0.1`.
pub const RR_SCHEMA_VERSION: &str = "rr-rsp-0.1";

/// Domain separation string for signatures. The trailing NUL is part of it.
pub const SIGNATURE_DOMAIN: &[u8] = b"semantic-core:sc-event:ed25519:v1\0";

/// Provenance class. Never promoted silently between classes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProvenanceKind {
    CompilerCertified,
    Declared,
    Derived,
    Observed,
}

impl ProvenanceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CompilerCertified => "compiler-certified",
            Self::Declared => "declared",
            Self::Derived => "derived",
            Self::Observed => "observed",
        }
    }
}

/// Disclosure level. This is a projection, not authentication.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AccessLevel {
    Public,
    Member,
    Owner,
    Administrator,
}

impl AccessLevel {
    pub const fn permits(self, required: Self) -> bool {
        self as u8 >= required as u8
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Member => "member",
            Self::Owner => "owner",
            Self::Administrator => "administrator",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    pub kind: ProvenanceKind,
    pub authority: String,
    pub source: String,
    pub source_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

impl Provenance {
    /// A non-observed provenance record.
    pub fn new(
        kind: ProvenanceKind,
        authority: impl Into<String>,
        source: impl Into<String>,
        source_hash: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            authority: authority.into(),
            source: source.into(),
            source_hash: source_hash.into(),
            observed_at: None,
            expires_at: None,
        }
    }

    /// An observed provenance record with its freshness window.
    pub fn observed(
        authority: impl Into<String>,
        source: impl Into<String>,
        source_hash: impl Into<String>,
        observed_at: impl Into<String>,
        expires_at: impl Into<String>,
    ) -> Self {
        Self {
            kind: ProvenanceKind::Observed,
            authority: authority.into(),
            source: source.into(),
            source_hash: source_hash.into(),
            observed_at: Some(observed_at.into()),
            expires_at: Some(expires_at.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Compatibility {
    pub protocol: String,
    pub subject_schema: String,
}

/// A closed body type carried by an envelope.
///
/// `project_for` removes body members below the given access level. The
/// default keeps the body unchanged; bodies with per-member access override it.
pub trait Body: Serialize + DeserializeOwned + Clone + PartialEq + fmt::Debug {
    fn project_for(&mut self, _access: AccessLevel) {}

    /// Body-specific validation, run as part of envelope validation.
    fn validate(&self) -> Result<(), ValidationError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope<B> {
    pub schema_version: String,
    pub subject_id: String,
    pub compatibility: Compatibility,
    pub provenance: Provenance,
    pub minimum_access: AccessLevel,
    pub body: B,
}

impl<B: Body> Envelope<B> {
    /// A new `sc-event-0.1` envelope, validated.
    pub fn new(
        subject_id: impl Into<String>,
        subject_schema: impl Into<String>,
        provenance: Provenance,
        minimum_access: AccessLevel,
        body: B,
    ) -> Result<Self, ValidationError> {
        let envelope = Self::with_protocol(
            SCHEMA_VERSION,
            subject_id,
            subject_schema,
            provenance,
            minimum_access,
            body,
        );
        envelope.validate()?;
        Ok(envelope)
    }

    /// An envelope under an explicit protocol identifier. Not validated; used
    /// for the Reflective Rust conformance vector and by `validate` callers.
    pub fn with_protocol(
        protocol: impl Into<String>,
        subject_id: impl Into<String>,
        subject_schema: impl Into<String>,
        provenance: Provenance,
        minimum_access: AccessLevel,
        body: B,
    ) -> Self {
        let protocol = protocol.into();
        Self {
            schema_version: protocol.clone(),
            subject_id: subject_id.into(),
            compatibility: Compatibility {
                protocol,
                subject_schema: subject_schema.into(),
            },
            provenance,
            minimum_access,
            body,
        }
    }

    /// Full validation: structure plus the `sc-event-0.1` protocol pin.
    pub fn validate(&self) -> Result<(), ValidationError> {
        self.validate_structure()?;
        if self.schema_version != SCHEMA_VERSION {
            return Err(ValidationError::UnsupportedSchema(self.schema_version.clone()));
        }
        Ok(())
    }

    /// Structural validation shared by every protocol this crate serializes:
    /// identifiers, hash shape, protocol consistency, the observation rule and
    /// body validation. Does not pin the protocol identifier.
    pub fn validate_structure(&self) -> Result<(), ValidationError> {
        require_identifier("subject_id", &self.subject_id)?;
        require_nonempty("provenance.authority", &self.provenance.authority)?;
        require_nonempty("provenance.source", &self.provenance.source)?;
        require_sha256("provenance.source_hash", &self.provenance.source_hash)?;
        require_nonempty("schema_version", &self.schema_version)?;
        if self.compatibility.protocol != self.schema_version {
            return Err(ValidationError::UnsupportedSchema(self.compatibility.protocol.clone()));
        }
        if self.schema_version != SCHEMA_VERSION && self.schema_version != RR_SCHEMA_VERSION {
            return Err(ValidationError::UnsupportedSchema(self.schema_version.clone()));
        }
        require_nonempty("compatibility.subject_schema", &self.compatibility.subject_schema)?;
        match self.provenance.kind {
            ProvenanceKind::Observed => {
                let observed_at =
                    parse_timestamp("provenance.observed_at", self.provenance.observed_at.as_deref())?;
                let expires_at =
                    parse_timestamp("provenance.expires_at", self.provenance.expires_at.as_deref())?;
                if expires_at <= observed_at {
                    return Err(ValidationError::InvalidFreshnessWindow);
                }
            }
            _ if self.provenance.observed_at.is_some() || self.provenance.expires_at.is_some() => {
                return Err(ValidationError::IncompatibleProvenance);
            }
            _ => {}
        }
        self.body.validate()
    }

    /// The envelope as seen at `access`, or `None` when the envelope itself is
    /// below that level. Body members are projected by the body.
    pub fn project_for(&self, access: AccessLevel) -> Option<Self> {
        if !access.permits(self.minimum_access) {
            return None;
        }
        let mut projected = self.clone();
        projected.body.project_for(access);
        Some(projected)
    }

    /// RFC 8785 canonical JSON of the validated envelope.
    pub fn canonical_json(&self) -> Result<String, CanonicalError> {
        self.validate_structure().map_err(CanonicalError::Validation)?;
        let bytes = serde_jcs::to_vec(self).map_err(CanonicalError::Serialization)?;
        String::from_utf8(bytes).map_err(CanonicalError::Utf8)
    }

    /// `sha256:<hex>` of the canonical bytes.
    pub fn canonical_hash(&self) -> Result<String, CanonicalError> {
        let bytes = self.canonical_json()?;
        Ok(hash_bytes(bytes.as_bytes()))
    }

    pub fn require_compatible(&self, other: &Self) -> Result<(), CompatibilityError> {
        if self.compatibility.protocol != other.compatibility.protocol {
            return Err(CompatibilityError::Protocol {
                expected: self.compatibility.protocol.clone(),
                actual: other.compatibility.protocol.clone(),
            });
        }
        if self.compatibility.subject_schema != other.compatibility.subject_schema {
            return Err(CompatibilityError::SubjectSchema {
                expected: self.compatibility.subject_schema.clone(),
                actual: other.compatibility.subject_schema.clone(),
            });
        }
        Ok(())
    }

    /// Observed envelopes are fresh inside `[observed_at, expires_at)`. Other
    /// provenance kinds are always fresh.
    pub fn require_fresh_at(&self, now: OffsetDateTime) -> Result<(), FreshnessError> {
        self.validate_structure().map_err(FreshnessError::Validation)?;
        if self.provenance.kind != ProvenanceKind::Observed {
            return Ok(());
        }
        let observed_at =
            parse_timestamp("provenance.observed_at", self.provenance.observed_at.as_deref())
                .map_err(FreshnessError::Validation)?;
        let expires_at = parse_timestamp("provenance.expires_at", self.provenance.expires_at.as_deref())
            .map_err(FreshnessError::Validation)?;
        if now < observed_at {
            return Err(FreshnessError::NotYetValid);
        }
        if now >= expires_at {
            return Err(FreshnessError::Expired);
        }
        Ok(())
    }

    /// Sign under `key_id`. The key identifier is part of the signed payload.
    pub fn sign(
        &self,
        key_id: impl Into<String>,
        signing_key: &SigningKey,
    ) -> Result<AuthenticatedEnvelope<B>, AuthenticationError> {
        let key_id = key_id.into();
        require_identifier("signature.key_id", &key_id).map_err(AuthenticationError::Validation)?;
        let payload = signature_payload(&key_id, self)?;
        let signature = signing_key.sign(&payload);
        Ok(AuthenticatedEnvelope {
            envelope: self.clone(),
            authentication: EnvelopeAuthentication {
                algorithm: SignatureAlgorithm::Ed25519,
                key_id,
                signature: URL_SAFE_NO_PAD.encode(signature.to_bytes()),
            },
        })
    }
}

/// `sha256:<hex>` of arbitrary bytes, the same shape the envelope uses for
/// its own identity and for `source_hash`.
pub fn hash_bytes(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SignatureAlgorithm {
    Ed25519,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvelopeAuthentication {
    pub algorithm: SignatureAlgorithm,
    pub key_id: String,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthenticatedEnvelope<B> {
    pub envelope: Envelope<B>,
    pub authentication: EnvelopeAuthentication,
}

impl<B: Body> AuthenticatedEnvelope<B> {
    pub fn verify(
        &self,
        expected_key_id: &str,
        verifying_key: &VerifyingKey,
    ) -> Result<(), AuthenticationError> {
        if self.authentication.key_id != expected_key_id {
            return Err(AuthenticationError::UnexpectedKey {
                expected: expected_key_id.into(),
                actual: self.authentication.key_id.clone(),
            });
        }
        let bytes = URL_SAFE_NO_PAD
            .decode(&self.authentication.signature)
            .map_err(|_| AuthenticationError::MalformedSignature)?;
        let signature = Signature::from_slice(&bytes).map_err(|_| AuthenticationError::MalformedSignature)?;
        let payload = signature_payload(&self.authentication.key_id, &self.envelope)?;
        verifying_key
            .verify_strict(&payload, &signature)
            .map_err(|_| AuthenticationError::InvalidSignature)
    }
}

fn signature_payload<B: Body>(key_id: &str, envelope: &Envelope<B>) -> Result<Vec<u8>, AuthenticationError> {
    let canonical = envelope.canonical_json().map_err(AuthenticationError::Canonical)?;
    let mut payload = Vec::with_capacity(SIGNATURE_DOMAIN.len() + key_id.len() + canonical.len() + 1);
    payload.extend_from_slice(SIGNATURE_DOMAIN);
    payload.extend_from_slice(key_id.as_bytes());
    payload.push(0);
    payload.extend_from_slice(canonical.as_bytes());
    Ok(payload)
}

pub fn require_nonempty(field: &'static str, value: &str) -> Result<(), ValidationError> {
    if value.trim().is_empty() {
        Err(ValidationError::Empty(field))
    } else {
        Ok(())
    }
}

pub fn require_identifier(field: &'static str, value: &str) -> Result<(), ValidationError> {
    require_nonempty(field, value)?;
    if value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.' | ':' | '/'))
    {
        Ok(())
    } else {
        Err(ValidationError::InvalidValue {
            field,
            value: value.to_string(),
        })
    }
}

pub fn require_sha256(field: &'static str, value: &str) -> Result<(), ValidationError> {
    let Some(digest) = value.strip_prefix("sha256:") else {
        return Err(ValidationError::InvalidValue {
            field,
            value: value.into(),
        });
    };
    if digest.len() == 64
        && digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(ValidationError::InvalidValue {
            field,
            value: value.into(),
        })
    }
}

pub fn parse_timestamp(field: &'static str, value: Option<&str>) -> Result<OffsetDateTime, ValidationError> {
    let value = value.ok_or(ValidationError::Empty(field))?;
    OffsetDateTime::parse(value, &Rfc3339).map_err(|_| ValidationError::InvalidValue {
        field,
        value: value.into(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ValidationError {
    #[error("{0} must not be empty")]
    Empty(&'static str),
    #[error("{field} has invalid value {value:?}")]
    InvalidValue { field: &'static str, value: String },
    #[error("unsupported schema version {0:?}")]
    UnsupportedSchema(String),
    #[error("body and provenance kind are incompatible")]
    IncompatibleProvenance,
    #[error("observation expiry must be later than observation time")]
    InvalidFreshnessWindow,
    #[error("{0}")]
    Body(String),
}

#[derive(Debug, thiserror::Error)]
pub enum CanonicalError {
    #[error(transparent)]
    Validation(ValidationError),
    #[error(transparent)]
    Serialization(serde_json::Error),
    #[error(transparent)]
    Utf8(std::string::FromUtf8Error),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CompatibilityError {
    #[error("protocol mismatch: expected {expected:?}, got {actual:?}")]
    Protocol { expected: String, actual: String },
    #[error("subject schema mismatch: expected {expected:?}, got {actual:?}")]
    SubjectSchema { expected: String, actual: String },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FreshnessError {
    #[error(transparent)]
    Validation(ValidationError),
    #[error("observation time is in the future")]
    NotYetValid,
    #[error("observation has expired")]
    Expired,
}

#[derive(Debug, thiserror::Error)]
pub enum AuthenticationError {
    #[error(transparent)]
    Validation(ValidationError),
    #[error(transparent)]
    Canonical(CanonicalError),
    #[error("unexpected signing key: expected {expected:?}, got {actual:?}")]
    UnexpectedKey { expected: String, actual: String },
    #[error("signature encoding is malformed")]
    MalformedSignature,
    #[error("signature verification failed")]
    InvalidSignature,
}

/// A small closed body used by this crate's own vectors: a labelled note with
/// per-member access levels, enough to exercise projection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoteBody {
    pub text: String,
    #[serde(default)]
    pub members: Vec<NoteMember>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoteMember {
    pub name: String,
    pub minimum_access: AccessLevel,
}

impl Body for NoteBody {
    fn project_for(&mut self, access: AccessLevel) {
        self.members.retain(|member| access.permits(member.minimum_access));
    }

    fn validate(&self) -> Result<(), ValidationError> {
        require_nonempty("body.text", &self.text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note() -> Envelope<NoteBody> {
        Envelope::new(
            "sc:note:1",
            "note-1",
            Provenance::new(
                ProvenanceKind::Declared,
                "owner-local",
                "fixture:note",
                hash_bytes(b"note"),
            ),
            AccessLevel::Member,
            NoteBody {
                text: "hello".into(),
                members: vec![
                    NoteMember {
                        name: "public".into(),
                        minimum_access: AccessLevel::Public,
                    },
                    NoteMember {
                        name: "owner".into(),
                        minimum_access: AccessLevel::Owner,
                    },
                ],
            },
        )
        .unwrap()
    }

    #[test]
    fn canonical_hash_is_stable_and_shaped() {
        let first = note().canonical_hash().unwrap();
        assert_eq!(first, note().canonical_hash().unwrap());
        assert_eq!(first.len(), "sha256:".len() + 64);
    }

    #[test]
    fn unknown_fields_are_rejected_everywhere() {
        let mut value: serde_json::Value = serde_json::to_value(note()).unwrap();
        value["extra"] = serde_json::json!(1);
        assert!(serde_json::from_value::<Envelope<NoteBody>>(value.clone()).is_err());
        value.as_object_mut().unwrap().remove("extra");
        value["provenance"]["extra"] = serde_json::json!(1);
        assert!(serde_json::from_value::<Envelope<NoteBody>>(value.clone()).is_err());
        value["provenance"].as_object_mut().unwrap().remove("extra");
        value["compatibility"]["extra"] = serde_json::json!(1);
        assert!(serde_json::from_value::<Envelope<NoteBody>>(value.clone()).is_err());
        value["compatibility"].as_object_mut().unwrap().remove("extra");
        value["body"]["extra"] = serde_json::json!(1);
        assert!(serde_json::from_value::<Envelope<NoteBody>>(value.clone()).is_err());
        value["body"].as_object_mut().unwrap().remove("extra");
        serde_json::from_value::<Envelope<NoteBody>>(value).unwrap();
    }

    #[test]
    fn projection_withholds_envelope_and_members() {
        let envelope = note();
        assert!(envelope.project_for(AccessLevel::Public).is_none());
        let member = envelope.project_for(AccessLevel::Member).unwrap();
        assert_eq!(member.body.members.len(), 1);
        assert_eq!(member.body.members[0].name, "public");
        let owner = envelope.project_for(AccessLevel::Owner).unwrap();
        assert_eq!(owner.body.members.len(), 2);
    }

    #[test]
    fn observed_requires_window_and_others_forbid_it() {
        let mut envelope = note();
        envelope.provenance.kind = ProvenanceKind::Observed;
        assert_eq!(envelope.validate(), Err(ValidationError::Empty("provenance.observed_at")));
        envelope.provenance.observed_at = Some("2026-10-10T10:00:00Z".into());
        assert_eq!(envelope.validate(), Err(ValidationError::Empty("provenance.expires_at")));
        envelope.provenance.expires_at = Some("2026-10-10T10:00:00Z".into());
        assert_eq!(envelope.validate(), Err(ValidationError::InvalidFreshnessWindow));
        envelope.provenance.expires_at = Some("2026-10-10T10:05:00Z".into());
        envelope.validate().unwrap();

        envelope.provenance.kind = ProvenanceKind::Derived;
        assert_eq!(envelope.validate(), Err(ValidationError::IncompatibleProvenance));
        envelope.provenance.observed_at = None;
        assert_eq!(envelope.validate(), Err(ValidationError::IncompatibleProvenance));
        envelope.provenance.expires_at = None;
        envelope.validate().unwrap();
    }

    #[test]
    fn freshness_window_is_half_open() {
        let mut envelope = note();
        envelope.provenance = Provenance::observed(
            "scanner",
            "fixture:scan",
            hash_bytes(b"scan"),
            "2026-10-10T10:00:00Z",
            "2026-10-10T10:05:00Z",
        );
        let at = |s: &str| OffsetDateTime::parse(s, &Rfc3339).unwrap();
        envelope.require_fresh_at(at("2026-10-10T10:00:00Z")).unwrap();
        envelope.require_fresh_at(at("2026-10-10T10:04:59Z")).unwrap();
        assert_eq!(
            envelope.require_fresh_at(at("2026-10-10T10:05:00Z")),
            Err(FreshnessError::Expired)
        );
        assert_eq!(
            envelope.require_fresh_at(at("2026-10-10T09:59:59Z")),
            Err(FreshnessError::NotYetValid)
        );
        let declared = note();
        declared.require_fresh_at(at("2030-01-01T00:00:00Z")).unwrap();
    }

    #[test]
    fn signature_rejects_tampering_key_substitution_and_rotation() {
        let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
        let mut signed = note().sign("owner-key:2026-10", &signing_key).unwrap();
        signed.verify("owner-key:2026-10", &signing_key.verifying_key()).unwrap();

        assert!(matches!(
            signed.verify("owner-key:other", &signing_key.verifying_key()),
            Err(AuthenticationError::UnexpectedKey { .. })
        ));

        let rotated = SigningKey::from_bytes(&[8_u8; 32]);
        assert!(matches!(
            signed.verify("owner-key:2026-10", &rotated.verifying_key()),
            Err(AuthenticationError::InvalidSignature)
        ));
        let resigned = note().sign("owner-key:2026-11", &rotated).unwrap();
        resigned.verify("owner-key:2026-11", &rotated.verifying_key()).unwrap();
        assert_eq!(resigned.envelope.subject_id, signed.envelope.subject_id);
        assert_eq!(
            resigned.envelope.canonical_hash().unwrap(),
            signed.envelope.canonical_hash().unwrap()
        );

        signed.envelope.subject_id = "sc:note:2".into();
        assert!(matches!(
            signed.verify("owner-key:2026-10", &signing_key.verifying_key()),
            Err(AuthenticationError::InvalidSignature)
        ));
        signed.envelope.subject_id = "sc:note:1".into();
        signed.authentication.signature.replace_range(0..1, "A");
        assert!(matches!(
            signed.verify("owner-key:2026-10", &signing_key.verifying_key()),
            Err(AuthenticationError::InvalidSignature) | Err(AuthenticationError::MalformedSignature)
        ));
    }

    #[test]
    fn protocol_pin_and_compatibility() {
        let mut other = note();
        note().require_compatible(&other).unwrap();
        other.compatibility.subject_schema = "note-2".into();
        assert!(matches!(
            note().require_compatible(&other),
            Err(CompatibilityError::SubjectSchema { .. })
        ));
        let mut foreign = note();
        foreign.schema_version = "rr-rsp-0.1".into();
        foreign.compatibility.protocol = "rr-rsp-0.1".into();
        assert_eq!(
            foreign.validate(),
            Err(ValidationError::UnsupportedSchema("rr-rsp-0.1".into()))
        );
        foreign.validate_structure().unwrap();
        let mut unknown = note();
        unknown.schema_version = "other-1".into();
        unknown.compatibility.protocol = "other-1".into();
        assert!(unknown.validate_structure().is_err());
    }
}
