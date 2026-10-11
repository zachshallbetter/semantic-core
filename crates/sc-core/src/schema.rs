//! SQLite schema. Every materialized table keeps the full record as JSON in
//! `doc` beside the columns queries filter on; all of them are rebuilt from
//! the event log by `Store::replay`.

pub const SCHEMA: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;
PRAGMA busy_timeout = 5000;

CREATE TABLE IF NOT EXISTS policy (
    id        TEXT PRIMARY KEY,
    authority TEXT NOT NULL,
    owner     TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS events (
    seq      INTEGER PRIMARY KEY AUTOINCREMENT,
    hash     TEXT NOT NULL UNIQUE,
    previous TEXT,
    origin   TEXT,
    subject  TEXT NOT NULL,
    at       TEXT NOT NULL,
    envelope TEXT NOT NULL
);
-- One successor per event: two writers cannot both chain to the same
-- predecessor. Libraries created before this index gain it on open, since
-- CREATE UNIQUE INDEX IF NOT EXISTS runs on every open; a library that
-- already holds a fork fails that statement and must be repaired by hand.
CREATE UNIQUE INDEX IF NOT EXISTS events_previous ON events(previous);
CREATE INDEX IF NOT EXISTS events_subject ON events(subject);
CREATE INDEX IF NOT EXISTS events_origin ON events(origin);

-- One row per identifier-bearing record, for access checks and search.
CREATE TABLE IF NOT EXISTS entities (
    id       TEXT PRIMARY KEY,
    class    TEXT NOT NULL,
    label    TEXT NOT NULL DEFAULT '',
    owner    TEXT NOT NULL,
    audience TEXT NOT NULL DEFAULT '[]'
);
CREATE INDEX IF NOT EXISTS entities_class ON entities(class);

CREATE TABLE IF NOT EXISTS things (
    id  TEXT PRIMARY KEY,
    doc TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS artifacts (
    id  TEXT PRIMARY KEY,
    doc TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS revisions (
    id           TEXT PRIMARY KEY,
    artifact_id  TEXT NOT NULL,
    content_hash TEXT,
    doc          TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS revisions_artifact ON revisions(artifact_id);
CREATE INDEX IF NOT EXISTS revisions_hash ON revisions(content_hash);

CREATE TABLE IF NOT EXISTS instances (
    id          TEXT PRIMARY KEY,
    artifact_id TEXT NOT NULL,
    revision_id TEXT NOT NULL,
    volume_uuid TEXT NOT NULL,
    document_id TEXT NOT NULL,
    path        TEXT NOT NULL,
    name        TEXT,
    size        INTEGER,
    modified_at TEXT,
    status      TEXT NOT NULL,
    doc         TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS instances_key ON instances(volume_uuid, document_id);
CREATE INDEX IF NOT EXISTS instances_revision ON instances(revision_id);

CREATE TABLE IF NOT EXISTS presentations (
    id                    TEXT PRIMARY KEY,
    container_revision_id TEXT NOT NULL,
    member_path           TEXT NOT NULL,
    member_size           INTEGER NOT NULL,
    member_hash           TEXT NOT NULL,
    volume_entity         TEXT NOT NULL,
    item_id               INTEGER NOT NULL,
    mount_epoch           INTEGER NOT NULL,
    status                TEXT NOT NULL,
    doc                   TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS presentations_member
    ON presentations(container_revision_id, member_path, member_size, member_hash);

CREATE TABLE IF NOT EXISTS relationships (
    id           TEXT PRIMARY KEY,
    predicate    TEXT NOT NULL,
    source_id    TEXT NOT NULL,
    target_id    TEXT NOT NULL,
    materializes INTEGER NOT NULL,
    state        TEXT NOT NULL,
    doc          TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS relationships_source ON relationships(source_id);
CREATE INDEX IF NOT EXISTS relationships_target ON relationships(target_id);
CREATE UNIQUE INDEX IF NOT EXISTS relationships_triple
    ON relationships(predicate, source_id, target_id);

CREATE TABLE IF NOT EXISTS assertions (
    id              TEXT PRIMARY KEY,
    relationship_id TEXT NOT NULL,
    polarity        TEXT NOT NULL,
    status          TEXT NOT NULL,
    doc             TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS assertions_relationship ON assertions(relationship_id);

CREATE TABLE IF NOT EXISTS observations (
    id  TEXT PRIMARY KEY,
    doc TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS resolutions (
    id             TEXT PRIMARY KEY,
    observation_id TEXT NOT NULL,
    instance_id    TEXT,
    doc            TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS resolutions_instance ON resolutions(instance_id);
"#;

/// Tables rebuilt by replay. The event log and policy are kept.
pub const MATERIALIZED_TABLES: &[&str] = &[
    "entities",
    "things",
    "artifacts",
    "revisions",
    "instances",
    "presentations",
    "relationships",
    "assertions",
    "observations",
    "resolutions",
];
