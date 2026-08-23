use std::path::{Path, PathBuf};

use sulo_testharness::golden::{GoldenOutcome, REASONER_VERSION, check_golden, closure, diff};
use sulo_testharness::load::load_file;

const SULO: &str = "../sulo/sulo.ttl";
const MUTANT: &str = "mutants/no-subproperty-containment.ttl";

/// Real SULO, loaded with an explicit prerequisite check.
///
/// Every test here reads `../sulo/sulo.ttl` by relative path, so a
/// checkout without the sulo repo as a sibling directory would
/// otherwise panic on `.unwrap()` with a bare `Io` error that reads
/// like a harness bug. `mutants/regenerate.sh` already guards the same
/// prerequisite with an explicit message; this is that message.
fn sulo_ontology() -> horned_owl::ontology::set::SetOntology<horned_owl::model::RcStr> {
    assert!(
        Path::new(SULO).is_file(),
        "{SULO} not found. These tests read real SULO by relative path, so the \
         sulo repo must be checked out as a sibling of sulo-testharness \
         (the same prerequisite mutants/regenerate.sh checks for)."
    );
    load_file(Path::new(SULO))
        .expect("real SULO should load")
        .ontology
}

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
    let onto = sulo_ontology();
    let a = closure(&onto).unwrap();
    let b = closure(&onto).unwrap();
    assert_eq!(a, b, "closure must be byte-identical across runs");
}

#[test]
fn the_closure_is_sorted() {
    let onto = sulo_ontology();
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
    let onto = sulo_ontology();
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
    let clean = closure(&sulo_ontology()).unwrap();
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
    let onto = sulo_ontology();

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
    let clean = sulo_ontology();
    let mutant = load_file(Path::new(MUTANT)).unwrap().ontology;

    let wrote = check_golden(&clean, &path, true);
    assert_eq!(wrote, GoldenOutcome::Rebaselined);

    match check_golden(&mutant, &path, false) {
        GoldenOutcome::Drift(d) => {
            assert!(
                d.contains(
                    "subObjectPropertyOf\thttps://w3id.org/sulo/hasPart\thttps://w3id.org/sulo/contains"
                ),
                "drift should name the lost hasPart -> contains edge: {d}"
            );
            assert!(
                d.contains(
                    "subObjectPropertyOf\thttps://w3id.org/sulo/isPartOf\thttps://w3id.org/sulo/isIn"
                ),
                "drift should name the lost isPartOf -> isIn edge: {d}"
            );
        }
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
    std::fs::write(
        &path,
        "# reasoner: rustdl v0.0.1\n# completeness_guaranteed: false\nsatisfiable\tx\ttrue\n",
    )
    .unwrap();

    let onto = sulo_ontology();
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

    let onto = sulo_ontology();
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
fn check_golden_errors_without_writing_when_no_golden_file_exists_and_accept_is_false() {
    // Critical fix: a missing golden file must NEVER be silently
    // written and treated as a pass. That would let a wrong path, or
    // a checkout missing `suites/`, silently disable the harness's
    // primary defence while still exiting 0.
    let path = scratch_golden("missing-file-no-accept");
    let _ = std::fs::remove_file(&path); // ensure it truly does not exist

    let onto = sulo_ontology();
    match check_golden(&onto, &path, false) {
        GoldenOutcome::Error(m) => {
            assert!(
                m.contains("--accept-golden"),
                "message should tell the operator how to create one deliberately: {m}"
            );
        }
        other => panic!("expected Error on a missing golden file, got {other:?}"),
    }

    assert!(
        !path.exists(),
        "a missing golden file must not be written as a side effect of checking it"
    );
}

#[test]
fn check_golden_requires_rebaseline_on_completeness_mismatch() {
    // Ruling 2's completeness flag is not just recorded, it is
    // compared: a flip in what the oracle could guarantee is a
    // genuine change in the oracle's strength, even at a fixed
    // reasoner version, and must not be silently absorbed into Match.
    let path = scratch_golden("completeness-mismatch");
    std::fs::write(
        &path,
        format!(
            "# reasoner: {REASONER_VERSION}
# completeness_guaranteed: true
"
        ),
    )
    .unwrap();

    let onto = sulo_ontology();
    // Real SULO is out-of-fragment, so its actual completeness_guaranteed
    // is false; the fixture above deliberately claims true, to differ.
    match check_golden(&onto, &path, false) {
        GoldenOutcome::RebaselineRequired(m) => {
            assert!(
                m.contains("completeness_guaranteed"),
                "message should name the completeness mismatch: {m}"
            );
        }
        other => panic!("expected RebaselineRequired on a completeness mismatch, got {other:?}"),
    }

    let _ = std::fs::remove_file(&path);
}

#[test]
fn the_committed_golden_file_matches_clean_sulo() {
    // Important fix: nothing in the repository otherwise reads
    // `suites/sulo.golden`. Without this test the committed artifact
    // is inert, a stale or wrongly regenerated golden file is
    // undetectable, and (combined with the missing-file fix above) a
    // CI job that lost `suites/` would recreate it and pass. This is
    // the strong form of `the_closure_is_deterministic`: that test
    // runs two calls in one process against one already-loaded
    // ontology and so cannot see load-path or cross-process
    // variation; this test goes through the real committed file on
    // disk, the same path an operator's `cargo run -- golden` takes.
    let onto = sulo_ontology();
    let outcome = check_golden(&onto, Path::new("suites/sulo.golden"), false);
    assert_eq!(
        outcome,
        GoldenOutcome::Match,
        "the committed suites/sulo.golden must match current SULO's closure. \
         If SULO changed deliberately, regenerate it with --accept-golden"
    );
}

#[test]
fn the_closure_records_completeness_and_undecided_pairs() {
    // Ruling 2: a golden file that reports only what was decided
    // implies more certainty than the run actually had. The header
    // must carry the completeness flag, and any undecided pair must
    // be visible in the body so drift in the oracle's reach is caught
    // like any other drift.
    let onto = sulo_ontology();
    let text = closure(&onto).unwrap();
    assert!(
        text.lines()
            .any(|l| l.starts_with("# completeness_guaranteed: ")),
        "the header must record whether completeness is guaranteed: {text}"
    );
}
