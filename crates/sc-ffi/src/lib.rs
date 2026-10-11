//! sc-ffi: a C ABI over `sc-envelope` and a minimal `sc-core` store surface,
//! for the Swift package. The header is `include/sc.h`.
//!
//! Implemented:
//!
//! - Envelope functions over an envelope given as JSON text with any JSON
//!   body: canonical bytes, canonical hash, validation (pinned to
//!   `sc-event-0.1`), signing with a 32-byte seed, verification with a 32-byte
//!   public key. Canonical bytes and hashes also accept the Reflective Rust
//!   protocol identifier, so the reference vector can be checked across the
//!   boundary; signing and verification do not.
//! - A store handle: open or create, add a thing, add a file, link, resolve a
//!   sighting, report a file gone, inspect, why, history, retract. Every
//!   store call acts as the library owner.
//!
//! Conventions: every function returns an `int32_t` code (`SC_OK` is 0) and
//! writes results through out-parameters; every `char *` the library hands
//! out is owned by the caller until `sc_string_free`; the message for the
//! last failing call on the current thread is available from
//! `sc_last_error`; no panic crosses the boundary (a caught panic is
//! `SC_ERR_INTERNAL`). Handles are not thread-safe; use one per thread.

use ed25519_dalek::{SigningKey, VerifyingKey};
use sc_core::sc_envelope::{self, AuthenticatedEnvelope, Body, Envelope};
use sc_core::*;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::ffi::{c_char, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;

pub const SC_OK: i32 = 0;
pub const SC_ERR_ARGUMENT: i32 = 1;
pub const SC_ERR_VALIDATION: i32 = 2;
pub const SC_ERR_NOT_FOUND: i32 = 3;
pub const SC_ERR_UNAUTHORIZED: i32 = 4;
pub const SC_ERR_CONFLICT: i32 = 5;
pub const SC_ERR_INTEGRITY: i32 = 6;
pub const SC_ERR_STORAGE: i32 = 7;
pub const SC_ERR_AUTHENTICATION: i32 = 8;
pub const SC_ERR_INTERNAL: i32 = 9;

thread_local! {
    static LAST_ERROR: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// An opaque store handle.
pub struct ScStore {
    store: Store,
    principal: Principal,
}

/// Any JSON value as an envelope body. The envelope's own fields still
/// reject unknown members; the body is the caller's.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
struct JsonBody(serde_json::Value);

impl Body for JsonBody {}

struct Failure {
    code: i32,
    message: String,
}

type FfiResult<T> = std::result::Result<T, Failure>;

fn fail<T>(code: i32, message: impl Into<String>) -> FfiResult<T> {
    Err(Failure {
        code,
        message: message.into(),
    })
}

impl From<sc_core::Error> for Failure {
    fn from(error: sc_core::Error) -> Self {
        use sc_core::Error as E;
        let code = match &error {
            E::NotFound(_) => SC_ERR_NOT_FOUND,
            E::Unauthorized(_) => SC_ERR_UNAUTHORIZED,
            E::Vocab(_) | E::Envelope(_) | E::Canonical(_) | E::Json(_) | E::Interchange(_) => SC_ERR_VALIDATION,
            E::Authentication(_) => SC_ERR_AUTHENTICATION,
            E::Sqlite(_) => SC_ERR_STORAGE,
            E::ChainBroken { .. } | E::Integrity(_) => SC_ERR_INTEGRITY,
            E::Conflict(_) => SC_ERR_CONFLICT,
        };
        Failure {
            code,
            message: error.to_string(),
        }
    }
}

impl From<sc_envelope::ValidationError> for Failure {
    fn from(error: sc_envelope::ValidationError) -> Self {
        Failure {
            code: SC_ERR_VALIDATION,
            message: error.to_string(),
        }
    }
}

impl From<sc_envelope::CanonicalError> for Failure {
    fn from(error: sc_envelope::CanonicalError) -> Self {
        Failure {
            code: SC_ERR_VALIDATION,
            message: error.to_string(),
        }
    }
}

impl From<sc_envelope::AuthenticationError> for Failure {
    fn from(error: sc_envelope::AuthenticationError) -> Self {
        let code = match &error {
            sc_envelope::AuthenticationError::Validation(_) | sc_envelope::AuthenticationError::Canonical(_) => {
                SC_ERR_VALIDATION
            }
            _ => SC_ERR_AUTHENTICATION,
        };
        Failure {
            code,
            message: error.to_string(),
        }
    }
}

impl From<serde_json::Error> for Failure {
    fn from(error: serde_json::Error) -> Self {
        Failure {
            code: SC_ERR_VALIDATION,
            message: format!("json: {error}"),
        }
    }
}

/// Run a body, convert its outcome to a code, record the message.
fn guard(body: impl FnOnce() -> FfiResult<()>) -> i32 {
    match catch_unwind(AssertUnwindSafe(body)) {
        Ok(Ok(())) => {
            LAST_ERROR.with(|slot| *slot.borrow_mut() = None);
            SC_OK
        }
        Ok(Err(failure)) => {
            LAST_ERROR.with(|slot| *slot.borrow_mut() = Some(failure.message));
            failure.code
        }
        Err(panic) => {
            let message = panic
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "panic".to_string());
            LAST_ERROR.with(|slot| *slot.borrow_mut() = Some(format!("internal: {message}")));
            SC_ERR_INTERNAL
        }
    }
}

/// # Safety
/// `pointer` is null or a NUL-terminated string valid for the call.
unsafe fn text<'a>(pointer: *const c_char, what: &str) -> FfiResult<&'a str> {
    if pointer.is_null() {
        return fail(SC_ERR_ARGUMENT, format!("{what} is null"));
    }
    match CStr::from_ptr(pointer).to_str() {
        Ok(value) => Ok(value),
        Err(_) => fail(SC_ERR_ARGUMENT, format!("{what} is not UTF-8")),
    }
}

/// # Safety
/// `out` is null or a valid pointer to write to.
unsafe fn give(out: *mut *mut c_char, value: String) -> FfiResult<()> {
    if out.is_null() {
        return fail(SC_ERR_ARGUMENT, "out pointer is null");
    }
    match CString::new(value) {
        Ok(owned) => {
            *out = owned.into_raw();
            Ok(())
        }
        Err(_) => fail(SC_ERR_INTERNAL, "result contained a NUL byte"),
    }
}

fn give_json<T: Serialize>(out: *mut *mut c_char, value: &T) -> FfiResult<()> {
    let text = serde_json::to_string(value)?;
    // SAFETY: `out` is checked inside `give`.
    unsafe { give(out, text) }
}

/// # Safety
/// `bytes` is null or points at 32 readable bytes.
unsafe fn key_bytes(bytes: *const u8, what: &str) -> FfiResult<[u8; 32]> {
    if bytes.is_null() {
        return fail(SC_ERR_ARGUMENT, format!("{what} is null"));
    }
    let slice = std::slice::from_raw_parts(bytes, 32);
    Ok(slice.try_into().expect("32 bytes"))
}

fn parse_envelope(json: &str) -> FfiResult<Envelope<JsonBody>> {
    Ok(serde_json::from_str(json)?)
}

// -------------------------------------------------------------- strings

/// Free a string returned by this library.
///
/// # Safety
/// `s` is null or a pointer previously returned by this library and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn sc_string_free(s: *mut c_char) {
    if !s.is_null() {
        drop(CString::from_raw(s));
    }
}

/// The message of the last failing call on this thread, or null. Caller frees.
#[no_mangle]
pub extern "C" fn sc_last_error() -> *mut c_char {
    LAST_ERROR.with(|slot| match slot.borrow().as_ref() {
        Some(message) => CString::new(message.clone())
            .map(CString::into_raw)
            .unwrap_or(std::ptr::null_mut()),
        None => std::ptr::null_mut(),
    })
}

/// The crate version, a static string the caller must not free.
#[no_mangle]
pub extern "C" fn sc_version() -> *const c_char {
    static VERSION: &[u8] = concat!(env!("CARGO_PKG_VERSION"), "\0").as_bytes();
    VERSION.as_ptr() as *const c_char
}

// ------------------------------------------------------------- envelope

/// RFC 8785 canonical bytes of an envelope given as JSON (structural
/// validation only, so the Reflective Rust vector canonicalizes too).
///
/// # Safety
/// Pointers follow the header's contract.
#[no_mangle]
pub unsafe extern "C" fn sc_envelope_canonical(envelope_json: *const c_char, out_canonical: *mut *mut c_char) -> i32 {
    guard(|| {
        let envelope = parse_envelope(text(envelope_json, "envelope_json")?)?;
        let canonical = envelope.canonical_json()?;
        give(out_canonical, canonical)
    })
}

/// `sha256:<hex>` of the canonical bytes.
///
/// # Safety
/// Pointers follow the header's contract.
#[no_mangle]
pub unsafe extern "C" fn sc_envelope_hash(envelope_json: *const c_char, out_hash: *mut *mut c_char) -> i32 {
    guard(|| {
        let envelope = parse_envelope(text(envelope_json, "envelope_json")?)?;
        let hash = envelope.canonical_hash()?;
        give(out_hash, hash)
    })
}

/// Full validation, pinned to `sc-event-0.1`.
///
/// # Safety
/// Pointers follow the header's contract.
#[no_mangle]
pub unsafe extern "C" fn sc_envelope_validate(envelope_json: *const c_char) -> i32 {
    guard(|| {
        let envelope = parse_envelope(text(envelope_json, "envelope_json")?)?;
        envelope.validate()?;
        Ok(())
    })
}

/// Sign with a 32-byte Ed25519 seed under `key_id`; returns the
/// authenticated envelope as JSON.
///
/// # Safety
/// Pointers follow the header's contract; `seed` points at 32 bytes.
#[no_mangle]
pub unsafe extern "C" fn sc_envelope_sign(
    envelope_json: *const c_char,
    key_id: *const c_char,
    seed: *const u8,
    out_signed_json: *mut *mut c_char,
) -> i32 {
    guard(|| {
        let envelope = parse_envelope(text(envelope_json, "envelope_json")?)?;
        let key_id = text(key_id, "key_id")?;
        let signing = SigningKey::from_bytes(&key_bytes(seed, "seed")?);
        let signed = envelope.sign(key_id, &signing)?;
        give_json(out_signed_json, &signed)
    })
}

/// Verify an authenticated envelope (JSON) under `key_id` with a 32-byte
/// Ed25519 public key.
///
/// # Safety
/// Pointers follow the header's contract; `public_key` points at 32 bytes.
#[no_mangle]
pub unsafe extern "C" fn sc_envelope_verify(signed_json: *const c_char, key_id: *const c_char, public_key: *const u8) -> i32 {
    guard(|| {
        let signed: AuthenticatedEnvelope<JsonBody> = serde_json::from_str(text(signed_json, "signed_json")?)?;
        let key_id = text(key_id, "key_id")?;
        let verifying = match VerifyingKey::from_bytes(&key_bytes(public_key, "public_key")?) {
            Ok(key) => key,
            Err(_) => return fail(SC_ERR_ARGUMENT, "public_key is not a valid Ed25519 point"),
        };
        signed.verify(key_id, &verifying)?;
        Ok(())
    })
}

// ---------------------------------------------------------------- store

/// # Safety
/// `handle` is null or a live handle from `sc_store_open`.
unsafe fn store<'a>(handle: *mut ScStore) -> FfiResult<&'a mut ScStore> {
    if handle.is_null() {
        return fail(SC_ERR_ARGUMENT, "store handle is null");
    }
    Ok(&mut *handle)
}

/// Open a library at `path`, creating it for `owner` when it does not exist.
///
/// # Safety
/// Pointers follow the header's contract.
#[no_mangle]
pub unsafe extern "C" fn sc_store_open(path: *const c_char, owner: *const c_char, out_store: *mut *mut ScStore) -> i32 {
    guard(|| {
        let path = text(path, "path")?;
        let owner = text(owner, "owner")?;
        if out_store.is_null() {
            return fail(SC_ERR_ARGUMENT, "out_store is null");
        }
        let store = Store::open_or_create(path, owner)?;
        let principal = store.owner_principal();
        *out_store = Box::into_raw(Box::new(ScStore { store, principal }));
        Ok(())
    })
}

/// Close a handle. Null is ignored.
///
/// # Safety
/// `handle` is null or a live handle not yet closed.
#[no_mangle]
pub unsafe extern "C" fn sc_store_close(handle: *mut ScStore) {
    if !handle.is_null() {
        drop(Box::from_raw(handle));
    }
}

/// # Safety
/// Pointers follow the header's contract.
#[no_mangle]
pub unsafe extern "C" fn sc_store_add_thing(handle: *mut ScStore, label: *const c_char, out_id: *mut *mut c_char) -> i32 {
    guard(|| {
        let sc = store(handle)?;
        let label = text(label, "label")?;
        let thing = sc.store.create_thing(&sc.principal, label, Default::default())?;
        give(out_id, thing.id)
    })
}

fn location_for(path: &str, volume: &str, document: &str) -> FfiResult<(Location, Option<String>)> {
    let metadata = std::fs::metadata(path).ok();
    let name = Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned());
    let location = Location {
        volume_uuid: volume.to_string(),
        document_id: document.to_string(),
        path: path.to_string(),
        name,
        size: metadata.as_ref().map(|m| m.len()),
        modified_at: None,
    };
    let hash = std::fs::read(path).ok().map(|bytes| sc_envelope::hash_bytes(&bytes));
    Ok((location, hash))
}

#[derive(Serialize)]
struct FileRecord {
    artifact: Artifact,
    revision: Revision,
    instance: Instance,
}

/// Register a file at `path` under the within-volume key `(volume, document)`.
/// The content hash is read from the file when it exists.
///
/// # Safety
/// Pointers follow the header's contract.
#[no_mangle]
pub unsafe extern "C" fn sc_store_add_file(
    handle: *mut ScStore,
    path: *const c_char,
    volume: *const c_char,
    document: *const c_char,
    out_json: *mut *mut c_char,
) -> i32 {
    guard(|| {
        let sc = store(handle)?;
        let path = text(path, "path")?;
        let (location, hash) = location_for(path, text(volume, "volume")?, text(document, "document")?)?;
        let label = location.name.clone().unwrap_or_else(|| path.to_string());
        let (artifact, revision, instance) =
            sc.store
                .register_file(&sc.principal, &label, location, hash, Default::default())?;
        give_json(
            out_json,
            &FileRecord {
                artifact,
                revision,
                instance,
            },
        )
    })
}

#[derive(Serialize)]
struct LinkRecord {
    relationship: Relationship,
    assertion: Assertion,
    state: RelationshipState,
}

/// Declare, propose and accept under the policy authority.
///
/// # Safety
/// Pointers follow the header's contract.
#[no_mangle]
pub unsafe extern "C" fn sc_store_link(
    handle: *mut ScStore,
    source_id: *const c_char,
    predicate: *const c_char,
    target_id: *const c_char,
    out_json: *mut *mut c_char,
) -> i32 {
    guard(|| {
        let sc = store(handle)?;
        let (relationship, assertion) = sc.store.link(
            &sc.principal,
            text(predicate, "predicate")?,
            text(source_id, "source_id")?,
            text(target_id, "target_id")?,
            vec![],
        )?;
        let state = sc.store.relationship_state(&sc.principal, &relationship.id)?;
        give_json(
            out_json,
            &LinkRecord {
                relationship,
                assertion,
                state,
            },
        )
    })
}

/// Resolve a sighting of `path` under `(volume, document)`; the content hash
/// is read from the file when it exists. Returns the resolution as JSON.
///
/// # Safety
/// Pointers follow the header's contract.
#[no_mangle]
pub unsafe extern "C" fn sc_store_resolve(
    handle: *mut ScStore,
    path: *const c_char,
    volume: *const c_char,
    document: *const c_char,
    out_json: *mut *mut c_char,
) -> i32 {
    guard(|| {
        let sc = store(handle)?;
        let path = text(path, "path")?;
        let (location, content_hash) = location_for(path, text(volume, "volume")?, text(document, "document")?)?;
        let resolution = sc.store.observe(
            &sc.principal,
            Seen {
                location,
                content_hash,
                source: "sc-ffi".into(),
                window: None,
            },
        )?;
        give_json(out_json, &resolution)
    })
}

/// Report that a known instance's location no longer holds it.
///
/// # Safety
/// Pointers follow the header's contract.
#[no_mangle]
pub unsafe extern "C" fn sc_store_gone(handle: *mut ScStore, instance_id: *const c_char, out_json: *mut *mut c_char) -> i32 {
    guard(|| {
        let sc = store(handle)?;
        let resolution = sc.store.observe_gone(&sc.principal, text(instance_id, "instance_id")?, "sc-ffi")?;
        give_json(out_json, &resolution)
    })
}

/// # Safety
/// Pointers follow the header's contract.
#[no_mangle]
pub unsafe extern "C" fn sc_store_inspect(handle: *mut ScStore, id: *const c_char, out_json: *mut *mut c_char) -> i32 {
    guard(|| {
        let sc = store(handle)?;
        let inspection = sc.store.inspect(&sc.principal, text(id, "id")?)?;
        give_json(out_json, &inspection)
    })
}

/// # Safety
/// Pointers follow the header's contract.
#[no_mangle]
pub unsafe extern "C" fn sc_store_why(handle: *mut ScStore, relationship_id: *const c_char, out_json: *mut *mut c_char) -> i32 {
    guard(|| {
        let sc = store(handle)?;
        let why = sc.store.why(&sc.principal, text(relationship_id, "relationship_id")?)?;
        give_json(out_json, &why)
    })
}

/// # Safety
/// Pointers follow the header's contract.
#[no_mangle]
pub unsafe extern "C" fn sc_store_history(handle: *mut ScStore, id: *const c_char, out_json: *mut *mut c_char) -> i32 {
    guard(|| {
        let sc = store(handle)?;
        let history = sc.store.history(&sc.principal, text(id, "id")?)?;
        give_json(out_json, &history)
    })
}

#[derive(Serialize)]
struct RetractRecord {
    event: Event,
    assertion: Assertion,
    state: RelationshipState,
}

/// Retract an assertion with a compensating event.
///
/// # Safety
/// Pointers follow the header's contract.
#[no_mangle]
pub unsafe extern "C" fn sc_store_retract(
    handle: *mut ScStore,
    assertion_id: *const c_char,
    reason: *const c_char,
    out_json: *mut *mut c_char,
) -> i32 {
    guard(|| {
        let sc = store(handle)?;
        let assertion_id = text(assertion_id, "assertion_id")?;
        let reason = if reason.is_null() { "retracted" } else { text(reason, "reason")? };
        let event = sc.store.retract_assertion(&sc.principal, assertion_id, reason)?;
        let assertion = sc.store.get_assertion(&sc.principal, assertion_id)?;
        let state = sc.store.relationship_state(&sc.principal, &assertion.relationship_id)?;
        give_json(
            out_json,
            &RetractRecord {
                event,
                assertion,
                state,
            },
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(text: &str) -> CString {
        CString::new(text).unwrap()
    }

    fn take(pointer: *mut c_char) -> String {
        assert!(!pointer.is_null());
        let value = unsafe { CStr::from_ptr(pointer) }.to_string_lossy().into_owned();
        unsafe { sc_string_free(pointer) };
        value
    }

    fn last_error() -> String {
        take(sc_last_error())
    }

    const RR_VECTOR: &str = r#"{"schema_version":"rr-rsp-0.1","subject_id":"rust:type:Empty","compatibility":{"protocol":"rr-rsp-0.1","subject_schema":"type-1"},"provenance":{"kind":"derived","authority":"reflective-rust-conformance","source":"vector:empty-type","source_hash":"sha256:503e43da59434586d85f21cb89b94362a54f394a6877f67fdfc60f715a07e924"},"minimum_access":"public","body":{"descriptor_kind":"type","descriptor":{"name":"Empty","kind":"struct","fields":[],"variants":[]}}}"#;

    #[test]
    fn golden_vector_hashes_across_the_boundary_but_does_not_sign() {
        let mut out: *mut c_char = std::ptr::null_mut();
        assert_eq!(unsafe { sc_envelope_hash(c(RR_VECTOR).as_ptr(), &mut out) }, SC_OK);
        assert_eq!(
            take(out),
            "sha256:7c98ac356aa59dd29e466c6f6e5927eb5afd2ad85f283d59d920de3f7bf05a18"
        );
        assert_eq!(unsafe { sc_envelope_validate(c(RR_VECTOR).as_ptr()) }, SC_ERR_VALIDATION);
        assert!(last_error().contains("rr-rsp-0.1"));
        let seed = [7u8; 32];
        let mut signed: *mut c_char = std::ptr::null_mut();
        assert_eq!(
            unsafe { sc_envelope_sign(c(RR_VECTOR).as_ptr(), c("k").as_ptr(), seed.as_ptr(), &mut signed) },
            SC_ERR_VALIDATION
        );
        assert!(signed.is_null());
    }

    #[test]
    fn sign_verify_and_tamper_through_the_boundary() {
        let envelope = r#"{"schema_version":"sc-event-0.1","subject_id":"sc:thing:1","compatibility":{"protocol":"sc-event-0.1","subject_schema":"note-1"},"provenance":{"kind":"declared","authority":"owner-local","source":"actor:ava","source_hash":"sha256:503e43da59434586d85f21cb89b94362a54f394a6877f67fdfc60f715a07e924"},"minimum_access":"owner","body":{"text":"hello"}}"#;
        let seed = [7u8; 32];
        let public = SigningKey::from_bytes(&seed).verifying_key().to_bytes();
        let mut signed: *mut c_char = std::ptr::null_mut();
        assert_eq!(
            unsafe { sc_envelope_sign(c(envelope).as_ptr(), c("k").as_ptr(), seed.as_ptr(), &mut signed) },
            SC_OK
        );
        let signed_json = take(signed);
        assert_eq!(
            unsafe { sc_envelope_verify(c(&signed_json).as_ptr(), c("k").as_ptr(), public.as_ptr()) },
            SC_OK
        );
        assert_eq!(
            unsafe { sc_envelope_verify(c(&signed_json).as_ptr(), c("other").as_ptr(), public.as_ptr()) },
            SC_ERR_AUTHENTICATION
        );
        let tampered = signed_json.replace("hello", "hullo");
        assert_eq!(
            unsafe { sc_envelope_verify(c(&tampered).as_ptr(), c("k").as_ptr(), public.as_ptr()) },
            SC_ERR_AUTHENTICATION
        );
        // Null and non-JSON arguments are codes, not crashes.
        assert_eq!(unsafe { sc_envelope_hash(std::ptr::null(), &mut signed) }, SC_ERR_ARGUMENT);
        assert_eq!(unsafe { sc_envelope_hash(c("{").as_ptr(), &mut signed) }, SC_ERR_VALIDATION);
        assert!(last_error().starts_with("json:"));
    }

    #[test]
    fn store_demonstration_through_the_boundary() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("library.sqlite");
        let a = dir.path().join("Warranty.txt");
        let b = dir.path().join("Manual.txt");
        std::fs::write(&a, "warranty").unwrap();
        std::fs::write(&b, "manual").unwrap();
        let mut handle: *mut ScStore = std::ptr::null_mut();
        assert_eq!(
            unsafe { sc_store_open(c(db.to_str().unwrap()).as_ptr(), c("ava").as_ptr(), &mut handle) },
            SC_OK
        );
        let mut out: *mut c_char = std::ptr::null_mut();
        let path = |p: &Path| c(p.to_str().unwrap());
        assert_eq!(
            unsafe { sc_store_add_file(handle, path(&a).as_ptr(), c("vol").as_ptr(), c("doc-a").as_ptr(), &mut out) },
            SC_OK
        );
        let file_a: serde_json::Value = serde_json::from_str(&take(out)).unwrap();
        assert_eq!(
            unsafe { sc_store_add_file(handle, path(&b).as_ptr(), c("vol").as_ptr(), c("doc-b").as_ptr(), &mut out) },
            SC_OK
        );
        let file_b: serde_json::Value = serde_json::from_str(&take(out)).unwrap();
        let artifact_a = file_a["artifact"]["id"].as_str().unwrap();
        let artifact_b = file_b["artifact"]["id"].as_str().unwrap();
        assert_eq!(
            unsafe {
                sc_store_link(
                    handle,
                    c(artifact_b).as_ptr(),
                    c("REFERENCES").as_ptr(),
                    c(artifact_a).as_ptr(),
                    &mut out,
                )
            },
            SC_OK
        );
        let link: serde_json::Value = serde_json::from_str(&take(out)).unwrap();
        assert_eq!(link["state"], "PRESENT");
        let relationship = link["relationship"]["id"].as_str().unwrap().to_string();
        let assertion = link["assertion"]["id"].as_str().unwrap().to_string();

        let moved = dir.path().join("Dishwasher warranty.txt");
        std::fs::rename(&a, &moved).unwrap();
        assert_eq!(
            unsafe { sc_store_resolve(handle, path(&moved).as_ptr(), c("vol").as_ptr(), c("doc-a").as_ptr(), &mut out) },
            SC_OK
        );
        let resolution: serde_json::Value = serde_json::from_str(&take(out)).unwrap();
        assert_eq!(resolution["outcome"], "RESOLVED");
        assert_eq!(resolution["rule"], "within-volume-key");

        assert_eq!(unsafe { sc_store_inspect(handle, c(&relationship).as_ptr(), &mut out) }, SC_OK);
        let inspection: serde_json::Value = serde_json::from_str(&take(out)).unwrap();
        assert_eq!(inspection["state"], "PRESENT");
        assert_eq!(
            unsafe { sc_store_retract(handle, c(&assertion).as_ptr(), std::ptr::null(), &mut out) },
            SC_OK
        );
        let retracted: serde_json::Value = serde_json::from_str(&take(out)).unwrap();
        assert_eq!(retracted["state"], "UNKNOWN");
        assert_eq!(unsafe { sc_store_history(handle, c(&relationship).as_ptr(), &mut out) }, SC_OK);
        let history: serde_json::Value = serde_json::from_str(&take(out)).unwrap();
        assert_eq!(history.as_array().unwrap().len(), 4);
        assert_eq!(unsafe { sc_store_inspect(handle, c("nope").as_ptr(), &mut out) }, SC_ERR_NOT_FOUND);
        assert!(last_error().contains("not found"));
        unsafe { sc_store_close(handle) };
        unsafe { sc_store_close(std::ptr::null_mut()) };
    }
}
