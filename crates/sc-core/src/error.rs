#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Absent, or withheld from this principal. The two are never told apart.
    #[error("not found: {0}")]
    NotFound(String),
    #[error("unauthorized: {0}")]
    Unauthorized(String),
    #[error(transparent)]
    Vocab(#[from] sc_vocab::VocabError),
    #[error(transparent)]
    Envelope(#[from] sc_envelope::ValidationError),
    #[error(transparent)]
    Canonical(#[from] sc_envelope::CanonicalError),
    #[error(transparent)]
    Authentication(#[from] sc_envelope::AuthenticationError),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("event chain broken at seq {seq}: {reason}")]
    ChainBroken { seq: i64, reason: String },
    #[error("integrity: {0}")]
    Integrity(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("interchange: {0}")]
    Interchange(String),
}

pub type Result<T> = std::result::Result<T, Error>;
