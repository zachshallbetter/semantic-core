//! Signed interchange of event envelopes (decision D4): the local log is
//! unsigned and hash-chained; events are signed with the owner's key when
//! they leave the device and verified when they arrive.
//!
//! Export is access-checked: a principal receives only events whose every
//! reference is visible to them. Import verifies every signature before
//! applying anything, applies the batch in one transaction, refuses a batch
//! that references an identifier it cannot find, and skips events it already
//! holds, so importing the same file twice is a no-op.

use crate::error::{Error, Result};
use crate::events::{Change, EventRecord};
use crate::model::Principal;
use crate::store::Store;
use ed25519_dalek::{SigningKey, VerifyingKey};
use rusqlite::{params, OptionalExtension};
use sc_envelope::AuthenticatedEnvelope;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

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
    pub exporter: String,
    pub key_id: String,
    pub events: Vec<AuthenticatedEnvelope<EventRecord>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ImportReport {
    pub applied: usize,
    pub skipped: usize,
}

impl Store {
    /// Sign every event the principal may see with `signing_key`.
    pub fn export(&self, principal: &Principal, key_id: &str, signing_key: &SigningKey) -> Result<Interchange> {
        let mut events = Vec::new();
        for event in self.all_events()? {
            let change = &event.envelope.body.change;
            if matches!(change, Change::PolicyEstablished { .. }) {
                continue;
            }
            if !self.change_visible(principal, change)? {
                continue;
            }
            events.push(event.envelope.sign(key_id, signing_key)?);
        }
        Ok(Interchange {
            context: INTERCHANGE_CONTEXT.into(),
            format: INTERCHANGE_FORMAT.into(),
            exported_at: self.now(),
            exporter: principal.actor.clone(),
            key_id: key_id.into(),
            events,
        })
    }

    /// Verify and apply an interchange file. All or nothing; idempotent.
    pub fn import(
        &mut self,
        principal: &Principal,
        interchange: &Interchange,
        verifying_key: &VerifyingKey,
    ) -> Result<ImportReport> {
        self.require_owner(principal)?;
        if interchange.format != INTERCHANGE_FORMAT {
            return Err(Error::Interchange(format!(
                "unsupported format {:?}",
                interchange.format
            )));
        }
        for (index, signed) in interchange.events.iter().enumerate() {
            signed
                .verify(&interchange.key_id, verifying_key)
                .map_err(|e| Error::Interchange(format!("event {index}: {e}")))?;
            signed.envelope.validate()?;
        }

        let at = self.now();
        let authority = self.authority.clone();
        let mut previous = self.last_hash()?;
        let mut report = ImportReport::default();
        let mut created: BTreeSet<String> = BTreeSet::new();
        let tx = self.conn.transaction()?;
        for (index, signed) in interchange.events.iter().enumerate() {
            let incoming_hash = signed.envelope.canonical_hash()?;
            let origin = signed.envelope.body.origin.clone().unwrap_or_else(|| incoming_hash.clone());
            if known_event(&tx, &incoming_hash)? || known_event(&tx, &origin)? {
                report.skipped += 1;
                continue;
            }
            let change = &signed.envelope.body.change;
            for id in change.referenced_ids() {
                if !created.contains(&id) && !known_id(&tx, &id)? {
                    return Err(Error::Interchange(format!(
                        "event {index} references {id}, which is neither in this library nor earlier in the file"
                    )));
                }
            }
            let record = EventRecord {
                previous: previous.clone(),
                actor: signed.envelope.body.actor.clone(),
                authority: authority.clone(),
                at: at.clone(),
                origin: Some(origin),
                change: change.clone(),
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
            created.extend(change.created_ids());
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
            "SELECT 1 FROM events WHERE hash = ?1 OR origin = ?1 LIMIT 1",
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
