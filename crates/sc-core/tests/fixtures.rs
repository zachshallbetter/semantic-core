//! Data-driven scenarios under `fixtures/{moves,copies,edits,deletes,ambiguous}`.

mod common;

#[test]
fn moves() {
    assert!(common::run_dir("moves") >= 2);
}

#[test]
fn copies() {
    assert!(common::run_dir("copies") >= 2);
}

#[test]
fn edits() {
    assert!(common::run_dir("edits") >= 1);
}

#[test]
fn deletes() {
    assert!(common::run_dir("deletes") >= 2);
}

#[test]
fn ambiguous() {
    assert!(common::run_dir("ambiguous") >= 2);
}
