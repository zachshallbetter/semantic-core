//! The Stage 1 demonstration through the binary: link two files, move one,
//! resolve and inspect the relationship, retract, inspect the history. Then
//! export, import into a second library, and read it as another principal.
//! Offline, no model, no server.

use std::path::Path;
use std::process::Command;

struct Cli {
    db: std::path::PathBuf,
}

impl Cli {
    fn run(&self, args: &[&str]) -> (bool, String, String) {
        let output = Command::new(env!("CARGO_BIN_EXE_sc"))
            .arg("--db")
            .arg(&self.db)
            .args(args)
            .output()
            .unwrap();
        (
            output.status.success(),
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    }

    fn ok(&self, args: &[&str]) -> String {
        let (success, stdout, stderr) = self.run(args);
        assert!(success, "sc {args:?} failed:\n{stdout}{stderr}");
        stdout
    }

    fn fails(&self, args: &[&str]) -> String {
        let (success, stdout, stderr) = self.run(args);
        assert!(!success, "sc {args:?} unexpectedly succeeded:\n{stdout}");
        stderr
    }
}

fn field(output: &str, prefix: &str) -> String {
    output
        .lines()
        .find_map(|line| line.strip_prefix(prefix))
        .unwrap_or_else(|| panic!("no line starting {prefix:?} in:\n{output}"))
        .trim()
        .to_string()
}

#[test]
fn stage_one_demonstration() {
    let dir = tempfile::tempdir().unwrap();
    let files = dir.path().join("Robin");
    std::fs::create_dir_all(files.join("Kitchen")).unwrap();
    let warranty = files.join("Kitchen/Warranty.txt");
    let manual = files.join("Kitchen/Manual.txt");
    std::fs::write(&warranty, "warranty for the dishwasher").unwrap();
    std::fs::write(&manual, "manual for the dishwasher").unwrap();
    let cli = Cli {
        db: dir.path().join("library.sqlite"),
    };

    cli.ok(&["init", "--owner", "ava"]);
    let thing = cli.ok(&["add-thing", "Dishwasher", "--audience", "adults"]).trim().to_string();
    let warranty_out = cli.ok(&[
        "add-file",
        warranty.to_str().unwrap(),
        "--volume",
        "vol-robin",
        "--document",
        "doc-warranty",
        "--audience",
        "adults",
    ]);
    let warranty_artifact = field(&warranty_out, "artifact ");
    let warranty_instance = field(&warranty_out, "instance ");
    let manual_out = cli.ok(&[
        "add-file",
        manual.to_str().unwrap(),
        "--volume",
        "vol-robin",
        "--document",
        "doc-manual",
    ]);
    let manual_artifact = field(&manual_out, "artifact ");

    // Link two files to the thing, and to each other.
    let link = cli.ok(&[
        "link",
        &warranty_artifact,
        "DOCUMENTS",
        &thing,
        "--evidence",
        "note=bought together",
    ]);
    let relationship = field(&link, "relationship ");
    let assertion = field(&link, "assertion ").split(' ').next().unwrap().to_string();
    assert_eq!(field(&link, "state "), "PRESENT");
    cli.ok(&["link", &manual_artifact, "DOCUMENTS", &thing]);
    cli.ok(&["link", &manual_artifact, "REFERENCES", &warranty_artifact]);

    // Move one file within the volume (same document id, new path).
    let moved = files.join("Kitchen/Dishwasher warranty.txt");
    std::fs::rename(&warranty, &moved).unwrap();
    let resolved = cli.ok(&[
        "move-resolve",
        moved.to_str().unwrap(),
        "--volume",
        "vol-robin",
        "--document",
        "doc-warranty",
    ]);
    assert!(resolved.starts_with("RESOLVED by rule within-volume-key"), "{resolved}");
    assert_eq!(field(&resolved, "instance "), warranty_instance);

    // Inspect: the relationship survived; the instance has the new path.
    let inspection = cli.ok(&["inspect", &thing]);
    let json: serde_json::Value = serde_json::from_str(&inspection).unwrap();
    assert_eq!(json["kind"], "thing");
    assert_eq!(json["relationships"].as_array().unwrap().len(), 2);
    let instance_json: serde_json::Value = serde_json::from_str(&cli.ok(&["inspect", &warranty_instance])).unwrap();
    assert_eq!(instance_json["instance"]["location"]["path"], moved.to_str().unwrap());
    assert_eq!(instance_json["instance"]["status"], "present");

    // Move across volumes: the old location is gone, a sighting elsewhere with
    // the same content resolves by the single-candidate rule.
    cli.ok(&["gone", &warranty_instance]);
    let elsewhere = dir.path().join("Leo");
    std::fs::create_dir_all(&elsewhere).unwrap();
    let archived = elsewhere.join("Warranty.txt");
    std::fs::rename(&moved, &archived).unwrap();
    let resolved = cli.ok(&[
        "move-resolve",
        archived.to_str().unwrap(),
        "--volume",
        "vol-leo",
        "--document",
        "doc-9",
    ]);
    assert!(
        resolved.starts_with("RESOLVED by rule content-hash-single-candidate-previous-gone"),
        "{resolved}"
    );

    // Why: assertion, evidence, resolution rule and history.
    let why = cli.ok(&["why", &relationship]);
    assert!(why.contains("state PRESENT"), "{why}");
    assert!(why.contains("evidence note: bought together"), "{why}");
    assert!(why.contains(&format!("assertion {assertion} accepted under owner-local")), "{why}");
    let why_json: serde_json::Value = serde_json::from_str(&cli.ok(&["why", &relationship, "--json"])).unwrap();
    assert_eq!(why_json["state"], "PRESENT");

    // Retract: a compensating event; the evidence and history remain.
    let retract = cli.ok(&["retract", &assertion, "--reason", "wrong appliance"]);
    assert!(retract.contains("compensates sha256:"), "{retract}");
    assert_eq!(field(&retract, "state "), "UNKNOWN");
    let why = cli.ok(&["why", &relationship]);
    assert!(why.contains("SUPPORTS retracted"), "{why}");
    assert!(why.contains("evidence note: bought together"), "{why}");
    assert!(why.contains(&format!("assertion {assertion} retracted (wrong appliance)")), "{why}");
    let history = cli.ok(&["history", &relationship]);
    assert_eq!(history.lines().count(), 4, "{history}");
    assert!(history.lines().filter(|l| l.contains(&format!("assertion {assertion}"))).count() >= 3, "{history}");
    assert!(why.lines().filter(|l| l.starts_with("  #")).count() >= 4, "{why}");
    assert!(cli.fails(&["retract", &assertion]).contains("already retracted"));

    // Another principal: the manual and its edge to the thing are owner-only.
    let as_robin = ["--as", "robin", "--audience", "adults"];
    let seen = cli.ok(&[&["inspect", &thing], &as_robin[..]].concat());
    let seen: serde_json::Value = serde_json::from_str(&seen).unwrap();
    assert_eq!(seen["relationships"].as_array().unwrap().len(), 1);
    assert!(cli
        .fails(&[&["inspect", &manual_artifact], &as_robin[..]].concat())
        .contains("not found"));
    assert!(cli
        .fails(&[&["add-thing", "Dryer"], &as_robin[..]].concat())
        .contains("unauthorized"));

    // Export with a seed from a file (never argv), import into a new library.
    let seed_file = dir.path().join("seed.hex");
    std::fs::write(&seed_file, "07".repeat(32)).unwrap();
    let export = dir.path().join("export.json");
    let exported = cli.ok(&[
        "export",
        "--out",
        export.to_str().unwrap(),
        "--key-id",
        "ava-key",
        "--key-file",
        seed_file.to_str().unwrap(),
    ]);
    assert!(exported.starts_with("exported "), "{exported}");
    let signing = ed25519_dalek::SigningKey::from_bytes(&[7_u8; 32]);
    let verifying: String = signing
        .verifying_key()
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let second = Cli {
        db: dir.path().join("second.sqlite"),
    };
    second.ok(&["init", "--owner", "ava"]);
    let report = second.ok(&[
        "import",
        export.to_str().unwrap(),
        "--key-id",
        "ava-key",
        "--verifying-key",
        &verifying,
    ]);
    assert!(report.contains("skipped 0"), "{report}");
    let again = second.ok(&[
        "import",
        export.to_str().unwrap(),
        "--key-id",
        "ava-key",
        "--verifying-key",
        &verifying,
    ]);
    assert!(again.starts_with("applied 0 "), "{again}");
    let why = second.ok(&["why", &relationship]);
    assert!(why.contains(&format!("assertion {assertion} retracted (wrong appliance)")), "{why}");
    assert!(second.ok(&["verify"]).contains("chain intact"));
    assert!(cli.ok(&["verify"]).contains("chain intact"));

    // Wrong key: refused.
    let wrong = "00".repeat(32);
    let third = Cli {
        db: dir.path().join("third.sqlite"),
    };
    third.ok(&["init", "--owner", "ava"]);
    assert!(third
        .fails(&["import", export.to_str().unwrap(), "--key-id", "ava-key", "--verifying-key", &wrong])
        .contains("interchange"));

    assert!(Path::new(&cli.db).exists());
}

#[test]
fn usage_and_missing_db() {
    let output = Command::new(env!("CARGO_BIN_EXE_sc")).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    let output = Command::new(env!("CARGO_BIN_EXE_sc"))
        .arg("inspect")
        .arg("x")
        .env_remove("SC_DB")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--db"));
}
