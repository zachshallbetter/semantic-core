//! Signed interchange of event envelopes (decision D4): the local log is
//! unsigned and hash-chained; events are signed with the owner's key when
//! they leave the device and verified when they arrive.
//!
//! Export is access-checked and projected: a principal receives only events
//! whose every reference is visible to them, only the events this library's
//! owner authored first hand (events that arrived by import keep their
//! original actor and are not re-shared under the owner's key), and every
//! audience set is replaced by the disclosure level that leaves the device
//! (D6). Each exported envelope names the local event it came from in
//! `origin`, so a round trip is idempotent. For the owner that is the stored
//! hash; for anyone else it is a per-library alias keyed by the library
//! secret, and `compensates` and an imported event's own `origin` are
//! aliased the same way, so no stored hash from any log reaches a non-owner.
//! Aliases of hashes and of origins are recorded so a returning file is
//! recognised.
//!
//! Import fails closed. Every event is verified against the key bound to its
//! actor before anything is applied; a file carrying a `PolicyEstablished`
//! event is refused whole; an `AssertionAccepted` event is never applied,
//! because acceptance is the local authority's decision and goes through
//! `accept_assertion`; the source event time is kept and the import time is
//! recorded beside it; a change owned by someone other than the local owner
//! is re-based onto the local owner and the source owner is recorded on the
//! event. The batch applies in one transaction, refuses dangling references,
//! and skips events already held.

use crate::error::{Error, Result};
use crate::events::{Change, EventRecord};
use crate::model::Principal;
use crate::store::Store;
use ed25519_dalek::{SigningKey, VerifyingKey};
use rusqlite::{params, OptionalExtension};
use sc_envelope::AuthenticatedEnvelope;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const INTERCHANGE_FORMAT: &str = "sc-interchange-0.1";
pub const INTERCHANGE_CONTEXT: &str = "https://github.com/zachshallbetter/semantic-core/interchange/0.1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Interchange {
    #[serde(rename = "@context")]
    pub context: String,
    #[serde(rename = "@type")]
    pub format: String,
    pub exported_at: String,
    /// The actor who signed every event in this file.
    pub exporter: String,
    pub key_id: String,
    pub events: Vec<AuthenticatedEnvelope<EventRecord>>,
}

/// Which verifying key each actor is trusted to sign with.
#[derive(Debug, Clone, Default)]
pub struct TrustedKeys {
    keys: BTreeMap<String, (String, VerifyingKey)>,
}

impl TrustedKeys {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn trust(mut self, actor: impl Into<String>, key_id: impl Into<String>, key: VerifyingKey) -> Self {
        self.keys.insert(actor.into(), (key_id.into(), key));
        self
    }

    pub fn get(&self, actor: &str) -> Option<&(String, VerifyingKey)> {
        self.keys.get(actor)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ImportReport {
    pub applied: usize,
    pub skipped: usize,
    /// `AssertionAccepted` events left unapplied; the assertions arrive as
    /// proposals for the local authority to accept.
    pub acceptances_withheld: usize,
    /// Events whose access blocks were re-based onto the local owner.
    pub rebased: usize,
}

impl Store {
    /// Sign the owner-authored events the principal may see with `signing_key`.
    pub fn export(&self, principal: &Principal, key_id: &str, signing_key: &SigningKey) -> Result<Interchange> {
        let mut events = Vec::new();
        for event in self.all_events()? {
            let change = &event.envelope.body.change;
            if matches!(change, Change::PolicyEstablished { .. }) {
                continue;
            }
            if event.envelope.body.actor != self.owner {
                continue;
            }
            if !self.change_visible(principal, change)? {
                continue;
            }
            let mut envelope = event.envelope.clone();
            envelope.body.previous = None;
            let is_owner = principal.actor == self.owner;
            // Stored hashes are aliased for non-owners; a value that is already
            // an alias is keyed by its minting library and travels unchanged,
            // so that library recognises it however many hops later.
            let name = |hash: &str| -> Result<String> {
                if is_owner || hash.starts_with("origin:") {
                    Ok(hash.to_string())
                } else {
                    self.alias_of(hash)
                }
            };
            envelope.body.origin = Some(match &event.envelope.body.origin {
                Some(origin) => name(origin)?,
                None => name(&event.hash)?,
            });
            if let Change::AssertionRetracted { compensates, .. } = &mut envelope.body.change {
                *compensates = name(compensates)?;
            }
            envelope.body.change.project_access();
            events.push(envelope.sign(key_id, signing_key)?);
        }
        Ok(Interchange {
            context: INTERCHANGE_CONTEXT.into(),
            format: INTERCHANGE_FORMAT.into(),
            exported_at: self.now(),
            exporter: self.owner.clone(),
            key_id: key_id.into(),
            events,
        })
    }

    /// Verify and apply an interchange file. All or nothing; idempotent.
    /// `adopt_owner` names the one foreign owner whose changes may be re-based
    /// onto this library's owner; without it a file naming another owner is
    /// refused whole.
    pub fn import(
        &mut self,
        principal: &Principal,
        interchange: &Interchange,
        keys: &TrustedKeys,
        adopt_owner: Option<&str>,
    ) -> Result<ImportReport> {
        self.require_owner(principal)?;
        if interchange.format != INTERCHANGE_FORMAT {
            return Err(Error::Interchange(format!("unsupported format {:?}", interchange.format)));
        }
        for (index, signed) in interchange.events.iter().enumerate() {
            let actor = &signed.envelope.body.actor;
            let (key_id, key) = keys
                .get(actor)
                .ok_or_else(|| Error::Interchange(format!("event {index}: no trusted key for actor {actor:?}")))?;
            signed
                .verify(key_id, key)
                .map_err(|e| Error::Interchange(format!("event {index} by {actor}: {e}")))?;
            signed.envelope.validate()?;
            if matches!(signed.envelope.body.change, Change::PolicyEstablished { .. }) {
                return Err(Error::Interchange(format!(
                    "event {index} would establish policy; a library's policy is never imported"
                )));
            }
        }

        let imported_at = self.now();
        let owner = self.owner.clone();
        let mut report = ImportReport::default();
        let mut created: BTreeSet<String> = BTreeSet::new();
        let tx = self.conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let mut previous = crate::store::last_hash_in(&tx)?;
        for (index, signed) in interchange.events.iter().enumerate() {
            let incoming_hash = signed.envelope.canonical_hash()?;
            let origin = signed.envelope.body.origin.clone().unwrap_or_else(|| incoming_hash.clone());
            if known_event(&tx, &incoming_hash)? || known_event(&tx, &origin)? {
                report.skipped += 1;
                continue;
            }
            let mut change = signed.envelope.body.change.clone();
            if matches!(change, Change::AssertionAccepted { .. }) {
                report.acceptances_withheld += 1;
                continue;
            }
            for id in change.referenced_ids() {
                if !created.contains(&id) && !known_id(&tx, &id)? {
                    return Err(Error::Interchange(format!(
                        "event {index} references {id}, which is neither in this library nor earlier in the file"
                    )));
                }
            }
            let foreign: BTreeSet<String> = change.owners().into_iter().filter(|o| o != &owner).collect();
            let source_owner = match (foreign.len(), adopt_owner) {
                (0, _) => None,
                (1, Some(adopt)) if foreign.contains(adopt) => {
                    change.rewrite_owner(&owner);
                    report.rebased += 1;
                    foreign.into_iter().next()
                }
                (1, _) => {
                    let name = foreign.into_iter().next().unwrap_or_default();
                    return Err(Error::Interchange(format!(
                        "event {index} is owned by {name:?}; this library is owned by {owner:?}. Pass adopt-owner {name:?} to re-base the file onto {owner:?}; the source owner is then recorded on every event"
                    )));
                }
                _ => {
                    return Err(Error::Interchange(format!(
                        "event {index} names more than one owner: {foreign:?}"
                    )))
                }
            };
            let record = EventRecord {
                previous: previous.clone(),
                actor: signed.envelope.body.actor.clone(),
                authority: signed.envelope.body.authority.clone(),
                at: signed.envelope.body.at.clone(),
                origin: Some(origin),
                imported_at: Some(imported_at.clone()),
                source_owner,
                change,
            };
            let window = signed
                .envelope
                .provenance
                .observed_at
                .clone()
                .zip(signed.envelope.provenance.expires_at.clone());
            let envelope = Self::envelope_with_access(
                record,
                signed.envelope.provenance.kind,
                window,
                signed.envelope.minimum_access,
            )?;
            let event = Self::insert_and_apply(&tx, envelope)?;
            created.extend(event.envelope.body.change.created_ids());
            previous = Some(event.hash);
            report.applied += 1;
        }
        tx.commit()?;
        Ok(report)
    }
}

fn known_event(tx: &rusqlite::Transaction<'_>, hash: &str) -> Result<bool> {
    Ok(tx
        .query_row(
            "SELECT 1 FROM events WHERE hash = ?1 OR origin = ?1
             UNION SELECT 1 FROM event_aliases WHERE alias = ?1 LIMIT 1",
            params![hash],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

fn known_id(tx: &rusqlite::Transaction<'_>, id: &str) -> Result<bool> {
    Ok(tx
        .query_row(
            "SELECT 1 FROM entities WHERE id = ?1 UNION SELECT 1 FROM resolutions WHERE id = ?1 LIMIT 1",
            params![id],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}
