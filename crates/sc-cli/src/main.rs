//! `sc`: the Stage 1 command line over `sc-core`.
//!
//! Implemented: `add-thing`, `add-file`, `link`, `move-resolve`, `inspect`,
//! `retract`, `export`, `import`, `why`, `history`, `verify`. Every command
//! takes `--db <path>` (or `SC_DB`) and acts as the library owner unless
//! `--as <actor> [--audience <label>]...` names another principal; reads are
//! access-checked for that principal and writes are refused.
//!
//! Secrets never travel on argv: the export signing seed is read from a file
//! named by `--key-file` or from stdin with `--key-stdin`; import takes the
//! public verifying key, which is not a secret.
//!
//! Not implemented: reading `NSURLDocumentIdentifierKey` or the volume UUID
//! from the filesystem. `add-file` and `move-resolve` take `--volume` and
//! `--document` explicitly, falling back to the device and inode numbers of
//! the file as a stand-in key when it exists on disk. That stand-in is
//! evidence, not the within-volume key decision D2 names; the Mac surface
//! supplies the real one.

use sc_core::*;
use std::result::Result;
use std::collections::BTreeSet;
use std::io::Read;
use std::process::ExitCode;

const USAGE: &str = "\
sc <command> [options]

  --db <path>                  library file (or SC_DB)
  --as <actor>                 act as this principal (default: the owner)
  --audience <label>           audience of --as; repeatable

commands
  init --owner <actor>
  add-thing <label> [--audience <label>]...
  add-file <path> [--label <text>] [--volume <uuid>] [--document <id>] [--hash sha256:<hex>] [--audience <label>]...
  link <source-id> <predicate> <target-id> [--evidence <kind>=<value>]... [--propose-only]
  accept <assertion-id> --authority <name>
  move-resolve <path> [--volume <uuid>] [--document <id>] [--hash sha256:<hex>] [--size n] [--mtime rfc3339]
  gone <instance-id>
  inspect <id>
  why <relationship-id>
  history <id>
  retract <assertion-id> [--reason <text>]
  export --out <file> --key-id <id> (--key-file <path> | --key-stdin)
  import <file> --key-id <id> --verifying-key <hex> [--actor <name>] [--adopt-owner <name>]
                               the key is trusted for --actor (default: the file's exporter) only;
                               accepted assertions arrive as proposals for the local authority;
                               a file owned by someone else is refused unless --adopt-owner names
                               that owner, and the re-basing is then recorded on every event
  history <id>                 for a relationship, includes every assertion's events
  verify
";

struct Args {
    command: String,
    positional: Vec<String>,
    flags: Vec<(String, String)>,
    switches: BTreeSet<String>,
}

impl Args {
    fn parse(raw: Vec<String>) -> Result<Self, String> {
        let mut iter = raw.into_iter();
        let mut command = None;
        let mut positional = Vec::new();
        let mut flags = Vec::new();
        let mut switches = BTreeSet::new();
        let takes_value = |name: &str| {
            !matches!(
                name,
                "propose-only" | "key-stdin" | "json" | "help"
            )
        };
        while let Some(arg) = iter.next() {
            if let Some(name) = arg.strip_prefix("--") {
                if takes_value(name) {
                    let value = iter.next().ok_or_else(|| format!("--{name} needs a value"))?;
                    flags.push((name.to_string(), value));
                } else {
                    switches.insert(name.to_string());
                }
            } else if command.is_none() {
                command = Some(arg);
            } else {
                positional.push(arg);
            }
        }
        let command = command.ok_or_else(|| USAGE.to_string())?;
        Ok(Self {
            command,
            positional,
            flags,
            switches,
        })
    }

    fn flag(&self, name: &str) -> Option<&str> {
        self.flags
            .iter()
            .rev()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    fn flags_named(&self, name: &str) -> Vec<String> {
        self.flags
            .iter()
            .filter(|(n, _)| n == name)
            .map(|(_, v)| v.clone())
            .collect()
    }

    fn required(&self, name: &str) -> Result<&str, String> {
        self.flag(name).ok_or_else(|| format!("--{name} is required"))
    }

    fn arg(&self, index: usize, what: &str) -> Result<&str, String> {
        self.positional
            .get(index)
            .map(String::as_str)
            .ok_or_else(|| format!("missing {what}"))
    }
}

fn main() -> ExitCode {
    let args = match Args::parse(std::env::args().skip(1).collect()) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    if args.switches.contains("help") || args.command == "help" {
        print!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("sc: {message}");
            ExitCode::FAILURE
        }
    }
}

fn db_path(args: &Args) -> Result<String, String> {
    args.flag("db")
        .map(str::to_string)
        .or_else(|| std::env::var("SC_DB").ok())
        .ok_or_else(|| "--db <path> or SC_DB is required".to_string())
}

fn open(args: &Args) -> Result<(Store, Principal), String> {
    let store = Store::open(db_path(args)?).map_err(|e| e.to_string())?;
    let principal = match args.flag("as") {
        Some(actor) => Principal::new(actor, args.flags_named("audience")),
        None => store.owner_principal(),
    };
    Ok((store, principal))
}

fn audience(args: &Args) -> BTreeSet<String> {
    args.flags_named("audience").into_iter().collect()
}

fn print_json<T: serde::Serialize>(value: &T) -> Result<(), String> {
    println!("{}", serde_json::to_string_pretty(value).map_err(|e| e.to_string())?);
    Ok(())
}

/// A location for a path. Explicit `--volume`/`--document` win; otherwise
/// the file's device and inode stand in when it exists.
fn location_for(args: &Args, path: &str) -> Result<Location, String> {
    let metadata = std::fs::metadata(path).ok();
    let (volume, document) = match (args.flag("volume"), args.flag("document")) {
        (Some(volume), Some(document)) => (volume.to_string(), document.to_string()),
        (volume, document) => {
            let metadata = metadata
                .as_ref()
                .ok_or_else(|| format!("{path} does not exist; pass --volume and --document"))?;
            let (dev, ino) = fallback_key(metadata)?;
            (
                volume.map(str::to_string).unwrap_or_else(|| format!("fs-dev:{dev}")),
                document.map(str::to_string).unwrap_or_else(|| format!("fs-ino:{ino}")),
            )
        }
    };
    let name = std::path::Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned());
    let size = args
        .flag("size")
        .map(|s| s.parse::<u64>().map_err(|e| format!("--size: {e}")))
        .transpose()?
        .or_else(|| metadata.as_ref().map(|m| m.len()));
    let modified_at = args.flag("mtime").map(str::to_string).or_else(|| {
        metadata.as_ref().and_then(|m| m.modified().ok()).and_then(|t| {
            time::OffsetDateTime::from(t)
                .format(&time::format_description::well_known::Rfc3339)
                .ok()
        })
    });
    Ok(Location {
        volume_uuid: volume,
        document_id: document,
        path: path.to_string(),
        name,
        size,
        modified_at,
    })
}

/// Device and inode as a stand-in within-volume key. Only unix exposes
/// them; elsewhere the caller must pass `--volume` and `--document`, since a
/// shared placeholder key would make every file resolve to every other.
#[cfg(unix)]
fn fallback_key(metadata: &std::fs::Metadata) -> Result<(u64, u64), String> {
    use std::os::unix::fs::MetadataExt;
    Ok((metadata.dev(), metadata.ino()))
}

#[cfg(not(unix))]
fn fallback_key(_metadata: &std::fs::Metadata) -> Result<(u64, u64), String> {
    Err("this platform exposes no stable file identifier; pass --volume and --document".to_string())
}

fn content_hash(args: &Args, path: &str) -> Result<Option<String>, String> {
    if let Some(hash) = args.flag("hash") {
        return Ok(Some(hash.to_string()));
    }
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(sc_envelope::hash_bytes(&bytes))),
        Err(_) => Ok(None),
    }
}

fn evidence(args: &Args) -> Result<Vec<Evidence>, String> {
    args.flags_named("evidence")
        .iter()
        .map(|item| {
            item.split_once('=')
                .map(|(kind, value)| Evidence::new(kind, value))
                .ok_or_else(|| format!("--evidence {item:?} is not kind=value"))
        })
        .collect()
}

fn read_seed(args: &Args) -> Result<ed25519_dalek::SigningKey, String> {
    let text = if args.switches.contains("key-stdin") {
        let mut text = String::new();
        std::io::stdin()
            .read_to_string(&mut text)
            .map_err(|e| e.to_string())?;
        text
    } else {
        let path = args
            .flag("key-file")
            .ok_or_else(|| "--key-file <path> or --key-stdin is required".to_string())?;
        std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?
    };
    let bytes = unhex(text.trim())?;
    let seed: [u8; 32] = bytes
        .try_into()
        .map_err(|_| "signing seed must be 32 bytes of hex".to_string())?;
    Ok(ed25519_dalek::SigningKey::from_bytes(&seed))
}

fn unhex(text: &str) -> Result<Vec<u8>, String> {
    if text.len() % 2 != 0 {
        return Err("hex value has odd length".into());
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).map_err(|e| e.to_string()))
        .collect()
}

fn run(args: &Args) -> Result<(), String> {
    let err = |e: Error| e.to_string();
    match args.command.as_str() {
        "init" => {
            let owner = args.required("owner")?;
            let store = Store::open_or_create(db_path(args)?, owner).map_err(err)?;
            println!("library owned by {} with authority {}", store.owner(), store.authority());
        }
        "add-thing" => {
            let label = args.arg(0, "label")?;
            let (mut store, p) = open(args)?;
            let thing = store.create_thing(&p, label, audience(args)).map_err(err)?;
            println!("{}", thing.id);
        }
        "add-file" => {
            let path = args.arg(0, "path")?;
            let (mut store, p) = open(args)?;
            let location = location_for(args, path)?;
            let label = args
                .flag("label")
                .map(str::to_string)
                .or_else(|| location.name.clone())
                .unwrap_or_else(|| path.to_string());
            let hash = content_hash(args, path)?;
            let (artifact, revision, instance) = store
                .register_file(&p, &label, location, hash, audience(args))
                .map_err(err)?;
            println!("artifact {}", artifact.id);
            println!("revision {}", revision.id);
            println!("instance {}", instance.id);
        }
        "link" => {
            let source = args.arg(0, "source id")?;
            let predicate = args.arg(1, "predicate")?;
            let target = args.arg(2, "target id")?;
            let (mut store, p) = open(args)?;
            let evidence = evidence(args)?;
            if args.switches.contains("propose-only") {
                let relationship = store
                    .declare_relationship(&p, predicate, source, target)
                    .map_err(err)?;
                let assertion = store
                    .propose_assertion(&p, &relationship.id, Polarity::Supports, evidence)
                    .map_err(err)?;
                println!("relationship {}", relationship.id);
                println!("assertion {} proposed", assertion.id);
                println!("state {}", store.relationship_state(&p, &relationship.id).map_err(err)?);
            } else {
                let (relationship, assertion) = store
                    .link(&p, predicate, source, target, evidence)
                    .map_err(err)?;
                println!("relationship {}", relationship.id);
                println!("assertion {} accepted by {}", assertion.id, store.authority());
                println!("state {}", store.relationship_state(&p, &relationship.id).map_err(err)?);
            }
        }
        "accept" => {
            let assertion = args.arg(0, "assertion id")?;
            let authority = args.required("authority")?;
            let (mut store, p) = open(args)?;
            let accepted = store.accept_assertion(&p, assertion, authority).map_err(err)?;
            println!("assertion {} {}", accepted.id, accepted.status.as_str());
            println!(
                "state {}",
                store.relationship_state(&p, &accepted.relationship_id).map_err(err)?
            );
        }
        "move-resolve" => {
            let path = args.arg(0, "path")?;
            let (mut store, p) = open(args)?;
            let location = location_for(args, path)?;
            let hash = content_hash(args, path)?;
            let resolution = store
                .observe(
                    &p,
                    Seen {
                        location,
                        content_hash: hash,
                        source: "sc move-resolve".into(),
                        window: None,
                    },
                )
                .map_err(err)?;
            println!("{} by rule {}", resolution.outcome, resolution.rule.as_str());
            if let Some(instance) = &resolution.instance_id {
                println!("instance {instance}");
            }
            for candidate in &resolution.candidates {
                println!("candidate {candidate}");
            }
            for item in &resolution.evidence {
                println!("evidence {}: {}", item.kind, item.value);
            }
        }
        "gone" => {
            let instance = args.arg(0, "instance id")?;
            let (mut store, p) = open(args)?;
            let resolution = store.observe_gone(&p, instance, "sc gone").map_err(err)?;
            println!("{} by rule {}", resolution.outcome, resolution.rule.as_str());
        }
        "inspect" => {
            let id = args.arg(0, "id")?;
            let (store, p) = open(args)?;
            let inspection = store.inspect(&p, id).map_err(err)?;
            print_json(&inspection)?;
        }
        "why" => {
            let id = args.arg(0, "relationship id")?;
            let (store, p) = open(args)?;
            let why = store.why(&p, id).map_err(err)?;
            if args.switches.contains("json") {
                print_json(&why)?;
            } else {
                print_why(&why);
            }
        }
        "history" => {
            let id = args.arg(0, "id")?;
            let (store, p) = open(args)?;
            for event in store.history(&p, id).map_err(err)? {
                println!("{}", describe_event(&event));
            }
        }
        "retract" => {
            let assertion = args.arg(0, "assertion id")?;
            let reason = args.flag("reason").unwrap_or("retracted");
            let (mut store, p) = open(args)?;
            let event = store.retract_assertion(&p, assertion, reason).map_err(err)?;
            let relationship = store.get_assertion(&p, assertion).map_err(err)?.relationship_id;
            println!("retracted by event {}", event.hash);
            if let Change::AssertionRetracted { compensates, .. } = &event.envelope.body.change {
                println!("compensates {compensates}");
            }
            println!("state {}", store.relationship_state(&p, &relationship).map_err(err)?);
        }
        "export" => {
            let out = args.required("out")?;
            let key_id = args.required("key-id")?;
            let (store, p) = open(args)?;
            let key = read_seed(args)?;
            let interchange = store.export(&p, key_id, &key).map_err(err)?;
            let text = serde_json::to_string_pretty(&interchange).map_err(|e| e.to_string())?;
            std::fs::write(out, text).map_err(|e| format!("{out}: {e}"))?;
            println!("exported {} events to {out}", interchange.events.len());
        }
        "import" => {
            let file = args.arg(0, "file")?;
            let key_id = args.required("key-id")?;
            let verifying = args.required("verifying-key")?;
            let (mut store, p) = open(args)?;
            let text = std::fs::read_to_string(file).map_err(|e| format!("{file}: {e}"))?;
            let interchange: Interchange = serde_json::from_str(&text).map_err(|e| e.to_string())?;
            if interchange.key_id != key_id {
                return Err(format!(
                    "file is signed by {:?}, not {key_id:?}",
                    interchange.key_id
                ));
            }
            let bytes: [u8; 32] = unhex(verifying)?
                .try_into()
                .map_err(|_| "verifying key must be 32 bytes of hex".to_string())?;
            let key = ed25519_dalek::VerifyingKey::from_bytes(&bytes).map_err(|e| e.to_string())?;
            let actor = args.flag("actor").unwrap_or(&interchange.exporter);
            let keys = TrustedKeys::new().trust(actor, key_id, key);
            let report = store
                .import(&p, &interchange, &keys, args.flag("adopt-owner"))
                .map_err(err)?;
            println!(
                "applied {} skipped {} acceptances withheld {} rebased {}",
                report.applied, report.skipped, report.acceptances_withheld, report.rebased
            );
        }
        "verify" => {
            let (store, p) = open(args)?;
            let count = store.verify_chain_as(&p).map_err(err)?;
            println!("chain intact over {count} events");
        }
        other => return Err(format!("unknown command {other}\n{USAGE}")),
    }
    Ok(())
}

fn print_why(why: &Why) {
    let r = &why.relationship;
    println!(
        "relationship {} : {} {} {} [{}]",
        r.id, r.source_id, r.predicate, r.target_id, r.vocab_version
    );
    println!("state {}", why.state);
    if !r.materializes {
        println!("not materialized: predicate is not in the registry");
    }
    for assertion in &why.assertions {
        println!(
            "assertion {} {} {} by {}{}{}",
            assertion.id,
            assertion.polarity.as_str(),
            assertion.status.as_str(),
            assertion.actor,
            assertion
                .accepted_by
                .as_deref()
                .map(|a| format!(" accepted under {a}"))
                .unwrap_or_default(),
            assertion
                .qualification
                .map(|q| format!(" qualified {}", q.as_str()))
                .unwrap_or_default()
        );
        for item in &assertion.evidence {
            println!("  evidence {}: {}", item.kind, item.value);
        }
    }
    for resolution in &why.resolutions {
        println!(
            "resolution {} {} by rule {}{}",
            resolution.id,
            resolution.outcome,
            resolution.rule.as_str(),
            resolution
                .instance_id
                .as_deref()
                .map(|i| format!(" -> instance {i}"))
                .unwrap_or_default()
        );
        for item in &resolution.evidence {
            println!("  evidence {}: {}", item.kind, item.value);
        }
    }
    println!("history");
    for event in &why.events {
        println!("  {}", describe_event(event));
    }
}

fn describe_event(event: &Event) -> String {
    let body = &event.envelope.body;
    let what = match &body.change {
        Change::PolicyEstablished { authority, .. } => format!("policy established: {authority}"),
        Change::ThingCreated { thing } => format!("thing created: {}", thing.label),
        Change::ArtifactCreated { artifact } => format!("artifact created: {}", artifact.label),
        Change::RevisionCreated { revision } => format!(
            "revision created: {}",
            revision.content_hash.as_deref().unwrap_or("no content hash")
        ),
        Change::InstanceRegistered { instance } => format!("instance registered at {}", instance.location.path),
        Change::InstanceUpdated { status, location, .. } => format!(
            "instance {}{}",
            status.as_str(),
            location
                .as_ref()
                .map(|l| format!(" at {}", l.path))
                .unwrap_or_default()
        ),
        Change::ObservationRecorded { resolution, .. } => {
            format!("observation {} by rule {}", resolution.outcome, resolution.rule.as_str())
        }
        Change::PresentationMounted { presentation } => format!("presentation mounted: {}", presentation.member.member_path),
        Change::PresentationRebound { .. } => "presentation re-bound".into(),
        Change::PresentationUnavailable { .. } => "presentation unavailable".into(),
        Change::RelationshipDeclared { relationship } => format!("relationship declared: {}", relationship.predicate),
        Change::AssertionProposed { assertion } => {
            format!("assertion {} proposed: {}", assertion.id, assertion.polarity.as_str())
        }
        Change::QualificationRecorded { assertion_id, qualification } => {
            format!("assertion {assertion_id} qualification recorded: {}", qualification.as_str())
        }
        Change::AssertionAccepted { assertion_id, authority } => {
            format!("assertion {assertion_id} accepted under {authority}")
        }
        Change::AssertionRetracted { assertion_id, reason, compensates } => {
            format!("assertion {assertion_id} retracted ({reason}), compensates {compensates}")
        }
        Change::AudienceChanged { audience, .. } => format!("audience changed: {audience:?}"),
    };
    format!(
        "#{} {} {} {} by {} [{}]",
        event.seq,
        body.at,
        event.envelope.provenance.kind.as_str(),
        what,
        body.actor,
        event.hash.get(..19).unwrap_or(&event.hash)
    )
}

#[cfg(test)]
mod tests {
    use super::fallback_key;

    #[cfg(unix)]
    #[test]
    fn unix_fallback_key_is_the_device_and_inode() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.txt");
        std::fs::write(&path, "a").unwrap();
        let metadata = std::fs::metadata(&path).unwrap();
        let (dev, ino) = fallback_key(&metadata).unwrap();
        assert_ne!((dev, ino), (0, 0));
    }

    #[cfg(not(unix))]
    #[test]
    fn other_platforms_refuse_to_derive_a_key() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.txt");
        std::fs::write(&path, "a").unwrap();
        let metadata = std::fs::metadata(&path).unwrap();
        let error = fallback_key(&metadata).unwrap_err();
        assert!(error.contains("--volume"));
    }
}
