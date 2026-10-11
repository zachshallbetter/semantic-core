//! `sc-vocab-1` relationship vocabulary (definition section 3, decision D3).
//!
//! Implemented and tested here:
//!
//! - The seven registered predicates with direction, source and target
//!   classes, and an inverse label where one is defined. `RELATED_TO` is
//!   symmetric; the other inverses are traversal labels, not storable
//!   predicates. Stored relationships always use the forward predicate.
//! - Class constraint checks on insert (`check`).
//! - Namespaced extension predicates (`x-<namespace>:NAME`) accepted without
//!   a registry change and without class constraints.
//! - Unregistered, un-namespaced predicates preserved as values under
//!   `x-unknown:<original>` with the vocabulary version they arrived under,
//!   and excluded from materialization (`Predicate::materializes`).
//!
//! Not implemented: version-to-version mappings (there is only one version),
//! and the reserved names `Episode`, time facets and known-absence records.

use serde::{Deserialize, Serialize};
use std::fmt;

/// The current vocabulary version.
pub const VOCAB_VERSION: &str = "sc-vocab-1";

/// Entity classes a predicate's ends may take.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntityClass {
    Thing,
    Artifact,
    Revision,
    Instance,
    Presentation,
}

impl EntityClass {
    pub const ALL: [EntityClass; 5] = [
        EntityClass::Thing,
        EntityClass::Artifact,
        EntityClass::Revision,
        EntityClass::Instance,
        EntityClass::Presentation,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Thing => "thing",
            Self::Artifact => "artifact",
            Self::Revision => "revision",
            Self::Instance => "instance",
            Self::Presentation => "presentation",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|class| class.as_str() == text)
    }
}

impl fmt::Display for EntityClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A registered predicate of `sc-vocab-1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Registered {
    InstanceOf,
    RevisionOf,
    Documents,
    References,
    DerivedFrom,
    Supersedes,
    RelatedTo,
}

/// Registry entry for one predicate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Definition {
    pub predicate: Registered,
    pub name: &'static str,
    /// Label used when the relationship is read from the target's side.
    pub inverse: Option<&'static str>,
    pub symmetric: bool,
    pub sources: &'static [EntityClass],
    pub targets: &'static [EntityClass],
}

use EntityClass::*;

const ANY: &[EntityClass] = &EntityClass::ALL;

const REGISTRY: &[Definition] = &[
    Definition {
        predicate: Registered::InstanceOf,
        name: "INSTANCE_OF",
        inverse: Some("HAS_INSTANCE"),
        symmetric: false,
        sources: &[Instance],
        targets: &[Revision, Artifact],
    },
    Definition {
        predicate: Registered::RevisionOf,
        name: "REVISION_OF",
        inverse: Some("HAS_REVISION"),
        symmetric: false,
        sources: &[Revision],
        targets: &[Artifact],
    },
    Definition {
        predicate: Registered::Documents,
        name: "DOCUMENTS",
        inverse: Some("DOCUMENTED_BY"),
        symmetric: false,
        sources: &[Artifact, Revision],
        targets: &[Thing, Artifact],
    },
    Definition {
        predicate: Registered::References,
        name: "REFERENCES",
        inverse: Some("REFERENCED_BY"),
        symmetric: false,
        sources: &[Artifact, Revision],
        targets: ANY,
    },
    Definition {
        predicate: Registered::DerivedFrom,
        name: "DERIVED_FROM",
        inverse: Some("DERIVATION_OF"),
        symmetric: false,
        sources: &[Instance, Artifact, Revision],
        targets: &[Instance, Artifact, Revision, Presentation],
    },
    Definition {
        predicate: Registered::Supersedes,
        name: "SUPERSEDES",
        inverse: Some("SUPERSEDED_BY"),
        symmetric: false,
        sources: &[Revision, Artifact],
        targets: &[Revision, Artifact],
    },
    Definition {
        predicate: Registered::RelatedTo,
        name: "RELATED_TO",
        inverse: Some("RELATED_TO"),
        symmetric: true,
        sources: ANY,
        targets: ANY,
    },
];

impl Registered {
    pub fn definition(self) -> &'static Definition {
        REGISTRY
            .iter()
            .find(|definition| definition.predicate == self)
            .expect("every registered predicate has a definition")
    }

    pub fn name(self) -> &'static str {
        self.definition().name
    }

    pub fn parse(text: &str) -> Option<Self> {
        REGISTRY
            .iter()
            .find(|definition| definition.name == text)
            .map(|definition| definition.predicate)
    }
}

/// The full registry, in declaration order.
pub fn registry() -> &'static [Definition] {
    REGISTRY
}

/// A predicate as stored on a relationship.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Predicate {
    /// A member of the registry.
    Registered(Registered),
    /// A namespaced extension such as `x-peel:MEMBER_OF`. Accepted, not
    /// class-checked, materialized.
    Extension { namespace: String, name: String },
    /// An unregistered, un-namespaced predicate preserved as a value. Never
    /// materialized until mapped.
    Unknown(String),
}

impl Predicate {
    /// Parse the stored spelling. `x-unknown:NAME` round-trips to `Unknown`.
    pub fn parse(text: &str) -> Result<Self, VocabError> {
        let text = text.trim();
        if text.is_empty() {
            return Err(VocabError::Empty);
        }
        if let Some(registered) = Registered::parse(text) {
            return Ok(Self::Registered(registered));
        }
        if let Some(rest) = text.strip_prefix("x-") {
            let Some((namespace, name)) = rest.split_once(':') else {
                return Err(VocabError::MalformedExtension(text.to_string()));
            };
            if namespace.is_empty() || name.is_empty() || !is_token(namespace) || !is_token(name) {
                return Err(VocabError::MalformedExtension(text.to_string()));
            }
            if namespace == "unknown" {
                return Ok(Self::Unknown(name.to_string()));
            }
            return Ok(Self::Extension {
                namespace: namespace.to_string(),
                name: name.to_string(),
            });
        }
        if !is_token(text) {
            return Err(VocabError::MalformedExtension(text.to_string()));
        }
        Ok(Self::Unknown(text.to_string()))
    }

    /// The stored spelling.
    pub fn as_value(&self) -> String {
        match self {
            Self::Registered(registered) => registered.name().to_string(),
            Self::Extension { namespace, name } => format!("x-{namespace}:{name}"),
            Self::Unknown(name) => format!("x-unknown:{name}"),
        }
    }

    /// Whether relationships under this predicate take part in the
    /// materialized graph.
    pub fn materializes(&self) -> bool {
        !matches!(self, Self::Unknown(_))
    }

    pub fn inverse_label(&self) -> Option<String> {
        match self {
            Self::Registered(registered) => registered.definition().inverse.map(str::to_string),
            _ => None,
        }
    }

    pub fn is_symmetric(&self) -> bool {
        matches!(self, Self::Registered(registered) if registered.definition().symmetric)
    }

    /// Class constraint check (decision D3). Extension and unknown predicates
    /// carry no constraints.
    pub fn check(&self, source: EntityClass, target: EntityClass) -> Result<(), VocabError> {
        let Self::Registered(registered) = self else {
            return Ok(());
        };
        let definition = registered.definition();
        if !definition.sources.contains(&source) {
            return Err(VocabError::SourceClass {
                predicate: definition.name,
                class: source,
            });
        }
        if !definition.targets.contains(&target) {
            return Err(VocabError::TargetClass {
                predicate: definition.name,
                class: target,
            });
        }
        Ok(())
    }
}

impl fmt::Display for Predicate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.as_value())
    }
}

impl Serialize for Predicate {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.as_value())
    }
}

impl<'de> Deserialize<'de> for Predicate {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Predicate::parse(&text).map_err(serde::de::Error::custom)
    }
}

fn is_token(text: &str) -> bool {
    text.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VocabError {
    #[error("predicate must not be empty")]
    Empty,
    #[error("malformed predicate {0:?}: extensions are x-<namespace>:NAME")]
    MalformedExtension(String),
    #[error("{predicate} does not accept a {class} as its source")]
    SourceClass {
        predicate: &'static str,
        class: EntityClass,
    },
    #[error("{predicate} does not accept a {class} as its target")]
    TargetClass {
        predicate: &'static str,
        class: EntityClass,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_the_seven_predicates_with_direction_and_inverses() {
        let names: Vec<&str> = registry().iter().map(|d| d.name).collect();
        assert_eq!(
            names,
            [
                "INSTANCE_OF",
                "REVISION_OF",
                "DOCUMENTS",
                "REFERENCES",
                "DERIVED_FROM",
                "SUPERSEDES",
                "RELATED_TO"
            ]
        );
        for definition in registry() {
            assert!(!definition.sources.is_empty());
            assert!(!definition.targets.is_empty());
            assert_eq!(Registered::parse(definition.name), Some(definition.predicate));
        }
        assert!(Predicate::parse("RELATED_TO").unwrap().is_symmetric());
        assert_eq!(
            Predicate::parse("DOCUMENTS").unwrap().inverse_label().as_deref(),
            Some("DOCUMENTED_BY")
        );
        // Inverse labels are not storable predicates.
        assert!(matches!(Predicate::parse("DOCUMENTED_BY"), Ok(Predicate::Unknown(_))));
    }

    #[test]
    fn class_constraints_are_checked() {
        let documents = Predicate::parse("DOCUMENTS").unwrap();
        documents.check(Artifact, Thing).unwrap();
        documents.check(Revision, Artifact).unwrap();
        assert_eq!(
            documents.check(Thing, Artifact),
            Err(VocabError::SourceClass { predicate: "DOCUMENTS", class: Thing })
        );
        assert_eq!(
            documents.check(Artifact, Instance),
            Err(VocabError::TargetClass { predicate: "DOCUMENTS", class: Instance })
        );
        Predicate::parse("INSTANCE_OF").unwrap().check(Instance, Revision).unwrap();
        assert!(Predicate::parse("INSTANCE_OF").unwrap().check(Revision, Instance).is_err());
        Predicate::parse("DERIVED_FROM").unwrap().check(Instance, Artifact).unwrap();
        Predicate::parse("RELATED_TO").unwrap().check(Presentation, Thing).unwrap();
    }

    #[test]
    fn extensions_are_accepted_and_unknowns_preserved() {
        let peel = Predicate::parse("x-peel:MEMBER_OF").unwrap();
        assert_eq!(
            peel,
            Predicate::Extension { namespace: "peel".into(), name: "MEMBER_OF".into() }
        );
        assert!(peel.materializes());
        peel.check(Thing, Thing).unwrap();
        assert_eq!(peel.as_value(), "x-peel:MEMBER_OF");

        let unknown = Predicate::parse("OWNS").unwrap();
        assert_eq!(unknown, Predicate::Unknown("OWNS".into()));
        assert!(!unknown.materializes());
        assert_eq!(unknown.as_value(), "x-unknown:OWNS");
        assert_eq!(Predicate::parse("x-unknown:OWNS").unwrap(), unknown);

        assert_eq!(Predicate::parse(""), Err(VocabError::Empty));
        assert!(matches!(Predicate::parse("x-peel"), Err(VocabError::MalformedExtension(_))));
        assert!(matches!(Predicate::parse("x-:NAME"), Err(VocabError::MalformedExtension(_))));
        assert!(matches!(Predicate::parse("has space"), Err(VocabError::MalformedExtension(_))));
    }

    #[test]
    fn predicates_serialize_as_their_value() {
        let json = serde_json::to_string(&Predicate::parse("OWNS").unwrap()).unwrap();
        assert_eq!(json, "\"x-unknown:OWNS\"");
        let back: Predicate = serde_json::from_str(&json).unwrap();
        assert_eq!(back, Predicate::Unknown("OWNS".into()));
        let registered: Predicate = serde_json::from_str("\"SUPERSEDES\"").unwrap();
        assert_eq!(registered, Predicate::Registered(Registered::Supersedes));
    }
}
