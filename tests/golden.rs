use std::path::{Path, PathBuf};

use sulo_testharness::golden::{GoldenOutcome, REASONER_VERSION, check_golden, closure, diff};
use sulo_testharness::load::load_file;

const SULO: &str = "../sulo/sulo.ttl";
const MUTANT: &str = "mutants/no-subproperty-containment.ttl";

/// A process-and-test-unique scratch path so parallel test threads
/// never collide on the same golden file.
fn scratch_golden(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "sulo-testharness-golden-{name}-{}.golden",
        std::process::id()
    ))
}

#[test]
fn the_closure_is_deterministic() {
    let onto = load_file(Path::new(SULO)).unwrap().ontology;
    let a = closure(&onto).unwrap();
    let b = closure(&onto).unwrap();
    assert_eq!(a, b, "closure must be byte-identical across runs");
}

#[test]
fn the_closure_is_sorted() {
    let onto = load_file(Path::new(SULO)).unwrap().ontology;
    let text = closure(&onto).unwrap();
    let body: Vec<&str> = text.lines().filter(|l| !l.starts_with('#')).collect();
    let mut sorted = body.clone();
    sorted.sort_unstable();
    assert_eq!(
        body, sorted,
        "closure lines must be sorted for a readable diff"
    );
}

#[test]
fn the_closure_records_known_entailments() {
    let onto = load_file(Path::new(SULO)).unwrap().ontology;
    let text = closure(&onto).unwrap();
    assert!(
        text.contains("subClassOf\thttps://w3id.org/sulo/StartTime\thttps://w3id.org/sulo/Object"),
        "the deep chain should appear in the closure"
    );
    assert!(
        text.contains("satisfiable\thttps://w3id.org/sulo/Process"),
        "every class's satisfiability should be recorded"
    );
}

#[test]
fn an_identical_closure_has_no_diff() {
    assert!(diff("a\nb\n", "a\nb\n").is_none());
}

#[test]
fn a_changed_closure_reports_the_lines() {
    let d = diff("a\nc\n", "a\nb\n").expect("should differ");
    assert!(
        d.contains('c') && d.contains('b'),
        "diff should name both sides: {d}"
    );
}

#[test]
fn dropping_an_axiom_changes_the_closure() {
    // The mechanism the golden file exists for: a regression that no
    // hand-written case asserts still shows up as drift.
    let clean = closure(&load_file(Path::new(SULO)).unwrap().ontology).unwrap();
    let mutant = closure(&load_file(Path::new(MUTANT)).unwrap().ontology).unwrap();
    assert_ne!(
        clean, mutant,
        "removing a subproperty axiom must move the closure"
    );
}

#[test]
fn check_golden_writes_and_then_matches() {
    // The path this repo's actual usage takes: --accept-golden writes
    // the file, a later plain run against the SAME ontology matches.
    // This exercises `check_golden` itself, not just its `diff`/`closure`
    // building blocks: a `diff` that always returned `None` (a stub)
    // would let `check_golden`'s Drift arm go completely untested if
    // this test, and the one below, did not exist.
    let path = scratch_golden("writes-and-matches");
    let onto = load_file(Path::new(SULO)).unwrap().ontology;

    let wrote = check_golden(&onto, &path, true);
    assert_eq!(wrote, GoldenOutcome::Rebaselined);

    let matched = check_golden(&onto, &path, false);
    assert_eq!(matched, GoldenOutcome::Match);

    let _ = std::fs::remove_file(&path);
}

#[test]
fn check_golden_reports_drift_for_a_real_mutant() {
    // The end-to-end mechanism the golden file exists for, exercised
    // through `check_golden` itself rather than only through `diff`.
    let path = scratch_golden("drift-for-mutant");
    let clean = load_file(Path::new(SULO)).unwrap().ontology;
    let mutant = load_file(Path::new(MUTANT)).unwrap().ontology;

    let wrote = check_golden(&clean, &path, true);
    assert_eq!(wrote, GoldenOutcome::Rebaselined);

    match check_golden(&mutant, &path, false) {
        GoldenOutcome::Drift(d) => assert!(!d.is_empty(), "drift report should not be empty"),
        other => panic!("expected Drift against a mutated ontology, got {other:?}"),
    }

    let _ = std::fs::remove_file(&path);
}

#[test]
fn check_golden_requires_rebaseline_on_reasoner_version_mismatch() {
    // A reasoner upgrade legitimately moves the closure: it must be
    // reported as re-baseline required, never as drift and never as a
    // silent pass.
    let path = scratch_golden("version-mismatch");
    std::fs::write(&path, "# reasoner: rustdl v0.0.1\nsatisfiable\tx\ttrue\n").unwrap();

    let onto = load_file(Path::new(SULO)).unwrap().ontology;
    match check_golden(&onto, &path, false) {
        GoldenOutcome::RebaselineRequired(m) => {
            assert!(
                m.contains("v0.0.1"),
                "message should name the stale version: {m}"
            );
            assert!(
                m.contains(REASONER_VERSION),
                "message should name the running version: {m}"
            );
        }
        other => panic!("expected RebaselineRequired on a version mismatch, got {other:?}"),
    }

    let _ = std::fs::remove_file(&path);
}

#[test]
fn check_golden_requires_rebaseline_when_header_is_missing() {
    let path = scratch_golden("missing-header");
    std::fs::write(&path, "satisfiable\tx\ttrue\n").unwrap();

    let onto = load_file(Path::new(SULO)).unwrap().ontology;
    assert!(
        matches!(
            check_golden(&onto, &path, false),
            GoldenOutcome::RebaselineRequired(_)
        ),
        "a golden file with no reasoner header must never be silently trusted"
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn the_closure_records_completeness_and_undecided_pairs() {
    // Ruling 2: a golden file that reports only what was decided
    // implies more certainty than the run actually had. The header
    // must carry the completeness flag, and any undecided pair must
    // be visible in the body so drift in the oracle's reach is caught
    // like any other drift.
    let onto = load_file(Path::new(SULO)).unwrap().ontology;
    let text = closure(&onto).unwrap();
    assert!(
        text.lines()
            .any(|l| l.starts_with("# completeness_guaranteed: ")),
        "the header must record whether completeness is guaranteed: {text}"
    );
}
