//! Every documented exit code, observed from the actual binary.
//!
//! Spec 5.4 has defined codes 0 to 5 since the design was written, and
//! `tests/verdict.rs` has pinned `verdict::exit_code` since the engine
//! plan. Neither proved the PROGRAM could produce them: until the
//! `run` subcommand existed, codes 1 and 3 were unreachable from the
//! binary, and a contract nothing can exercise is this project's
//! recurring defect shape (a check that cannot fail) wearing the
//! clothes of a documented interface.
//!
//! So every row here launches `CARGO_BIN_EXE_sulo-testharness` and
//! asserts the observed status. A unit test over the mapping function
//! is not a substitute: it cannot catch a `main` that forgets to
//! propagate, that aggregates the wrong set, or that prints a report
//! and returns success anyway.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use sulo_testharness::verdict::{IndeterminateReason, Verdict, exit_code, run_exit_code};

const BIN: &str = env!("CARGO_BIN_EXE_sulo-testharness");
const SUITE: &str = "suites/sulo";
const SULO: &str = "../sulo/sulo.ttl";

/// Guard the prerequisite the same way `tests/mutation.rs` and the
/// group tests do, so a missing sibling checkout is a clear message
/// rather than a confusing exit 2.
fn require_sulo() {
    assert!(
        Path::new(SULO).is_file(),
        "{SULO} must exist: clone https://github.com/AIDAVA-DEV/sulo as a sibling directory"
    );
}

fn run(args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .output()
        .expect("the harness binary should be launchable")
}

fn status_of(args: &[&str]) -> i32 {
    run(args)
        .status
        .code()
        .expect("the process should exit normally, not by signal")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "sulo-testharness-cli-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir should be creatable");
    dir
}

// ---------------------------------------------------------------
// 0: the consumer's happy path.
// ---------------------------------------------------------------

/// The row that matters most: the real 66-case suite against real,
/// healthy SULO exits 0. This is precisely what the composite action
/// runs, so if it is ever non-zero the action is red out of the box
/// for every consumer.
///
/// It was 1 until `oracle-hermit` deferral landed, because
/// `timeinstant-datarange` asserts an axiom the pinned reasoner cannot
/// enforce. Slow (a real reasoner over 66 cases), and kept anyway: a
/// faster proxy would not prove the thing this asserts.
#[test]
fn the_real_suite_against_clean_sulo_exits_zero() {
    require_sulo();
    let out = run(&["run", "--suite", SUITE, "--ontology", SULO]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        out.status.code(),
        Some(0),
        "the real suite against healthy SULO must exit 0, or the action is red for every \
         consumer. stdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Deferral must be visible, not silent: a case excluded from the
    // exit code has to be named in the report a human reads.
    assert!(
        stdout.contains("DEFERRED") && stdout.contains("timeinstant-datarange"),
        "the deferred case must be named and counted in the report, not silently dropped:\n{stdout}"
    );
}

// ---------------------------------------------------------------
// 1: a real regression.
// ---------------------------------------------------------------

/// A mutant a case catches must exit 1. Narrowed with `--filter` to
/// the group that catches this mutant, so the test proves the code
/// path without paying for a second full-suite reasoner run.
#[test]
fn a_caught_mutant_exits_one() {
    let mutant = "mutants/no-feature-object.ttl";
    assert!(Path::new(mutant).is_file(), "{mutant} should exist");
    assert_eq!(
        status_of(&[
            "run",
            "--suite",
            SUITE,
            "--ontology",
            mutant,
            "--filter",
            "taxonomy/asserted-subsumptions",
        ]),
        1,
        "a case that catches this mutant must make the run exit 1"
    );
}

// ---------------------------------------------------------------
// 2: configuration errors, which are never claims about the ontology.
// ---------------------------------------------------------------

#[test]
fn a_suite_root_with_no_cases_exits_two() {
    let dir = scratch("empty");
    assert_eq!(
        status_of(&["run", "--suite", dir.to_str().unwrap(), "--ontology", SULO]),
        2,
        "an empty suite root must be a configuration error, never a green run over zero cases"
    );
}

#[test]
fn a_filter_matching_nothing_exits_two() {
    require_sulo();
    assert_eq!(
        status_of(&[
            "run",
            "--suite",
            SUITE,
            "--ontology",
            SULO,
            "--filter",
            "no-such-case-anywhere",
        ]),
        2,
        "a filter matching nothing must be a configuration error, not a pass over zero cases"
    );
}

#[test]
fn a_malformed_manifest_exits_two() {
    let dir = scratch("malformed");
    std::fs::write(
        dir.join("broken.yaml"),
        "id: broken\ndescription: d\nnot_a_key: 1\n",
    )
    .expect("case should be writable");
    assert_eq!(
        status_of(&["run", "--suite", dir.to_str().unwrap(), "--ontology", SULO]),
        2,
        "a manifest that does not load must abort the run: broken YAML is not evidence \
         about the ontology"
    );
}

/// A stray `*.yml` is refused BY NAME, and this test has to be able to
/// tell that refusal apart from the empty-suite guard, which also
/// exits 2. So the scratch suite also holds a perfectly good `.yaml`
/// case: without the refusal the run would find that case and exit 0,
/// and the assertion below pins the message naming the `.yml` file.
///
/// The first version of this test wrote only the `.yml` and asserted
/// the code alone. Removing the refusal left it green, because an
/// undiscovered `.yml` makes the suite empty and the empty-suite guard
/// returns exit 2 as well: a check that could not fail, caught by
/// mutating the code rather than by reading it.
#[test]
fn a_stray_yml_is_refused_by_name_not_merely_by_leaving_the_suite_empty() {
    let dir = scratch("stray-yml");
    std::fs::copy("tests/fixtures/clean.ttl", dir.join("clean.ttl"))
        .expect("the clean fixture should exist");
    std::fs::write(
        dir.join("good.yaml"),
        "id: good\ndescription: A valid case, so the suite is not empty.\nontology: clean.ttl\nunsatisfiable:\n  - owl:Nothing\n",
    )
    .expect("case should be writable");
    std::fs::write(
        dir.join("case.yml"),
        "id: c\ndescription: d\nunsatisfiable: [owl:Nothing]\n",
    )
    .expect("case should be writable");

    let out = run(&["run", "--suite", dir.to_str().unwrap()]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(2),
        "a *.yml would be discovered by nobody and reported by nothing, so it is refused \
         rather than silently skipped. stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("case.yml") && stderr.contains(".yaml"),
        "the refusal must name the offending file and the remedy, so it cannot be confused \
         with the empty-suite guard which also exits 2. stderr:\n{stderr}"
    );
}

// ---------------------------------------------------------------
// 3: the reasoner could not answer.
// ---------------------------------------------------------------

/// An unbound prefix cannot be expanded, so the check cannot be asked
/// at all: Indeterminate, exit 3. Not a Fail, because nothing about
/// the ontology was established either way.
#[test]
fn an_indeterminate_case_exits_three() {
    let dir = scratch("indeterminate");
    std::fs::copy("tests/fixtures/clean.ttl", dir.join("clean.ttl"))
        .expect("the clean fixture should exist");
    std::fs::write(
        dir.join("case.yaml"),
        "id: unbound-prefix\ndescription: An unbound prefix, so the check cannot be asked.\nontology: clean.ttl\nunsatisfiable:\n  - nope:Missing\n",
    )
    .expect("case should be writable");
    assert_eq!(
        status_of(&["run", "--suite", dir.to_str().unwrap()]),
        3,
        "a check that cannot be asked is Indeterminate, exit 3"
    );
}

/// Spec 5.4's escape hatch: `--allow-indeterminate` lowers 3 to 0.
#[test]
fn allow_indeterminate_lowers_three_to_zero() {
    let dir = scratch("allow-indeterminate");
    std::fs::copy("tests/fixtures/clean.ttl", dir.join("clean.ttl"))
        .expect("the clean fixture should exist");
    std::fs::write(
        dir.join("case.yaml"),
        "id: unbound-prefix\ndescription: An unbound prefix, so the check cannot be asked.\nontology: clean.ttl\nunsatisfiable:\n  - nope:Missing\n",
    )
    .expect("case should be writable");
    assert_eq!(
        status_of(&[
            "run",
            "--suite",
            dir.to_str().unwrap(),
            "--allow-indeterminate",
        ]),
        0,
        "--allow-indeterminate must lower 3 to 0 when no Fail is present (spec 5.4)"
    );
}

/// The direction that matters: the flag must never turn a real
/// regression green. A run holding BOTH a Fail and an Indeterminate
/// still exits 1.
#[test]
fn allow_indeterminate_never_suppresses_a_fail() {
    let dir = scratch("allow-indeterminate-fail");
    std::fs::copy("tests/fixtures/clean.ttl", dir.join("clean.ttl"))
        .expect("the clean fixture should exist");
    std::fs::write(
        dir.join("indeterminate.yaml"),
        "id: unbound-prefix\ndescription: An unbound prefix, so the check cannot be asked.\nontology: clean.ttl\nunsatisfiable:\n  - nope:Missing\n",
    )
    .expect("case should be writable");
    std::fs::write(
        dir.join("failing.yaml"),
        "id: cannot-hold\ndescription: Clean SULO is consistent, so this expectation fails.\nontology: clean.ttl\nexpect_inconsistent: true\n",
    )
    .expect("case should be writable");
    assert_eq!(
        status_of(&[
            "run",
            "--suite",
            dir.to_str().unwrap(),
            "--allow-indeterminate",
        ]),
        1,
        "--allow-indeterminate must never suppress a Fail"
    );
}

// ---------------------------------------------------------------
// 4: golden drift.
// ---------------------------------------------------------------

/// `no-feature-object` is one of the two mutants the class-only golden
/// closure can actually see (the measurement is in `src/golden.rs`).
#[test]
fn golden_drift_exits_four() {
    let mutant = "mutants/no-feature-object.ttl";
    assert!(Path::new(mutant).is_file(), "{mutant} should exist");
    assert_eq!(
        status_of(&[
            "golden",
            "--ontology",
            mutant,
            "--golden",
            "suites/sulo.golden",
        ]),
        4,
        "a mutant the closure sees must be reported as drift, exit 4"
    );
}

// ---------------------------------------------------------------
// 5: not reachable yet, and this says so out loud.
// ---------------------------------------------------------------

/// Exit 5 is oracle divergence between rustdl and HermiT (spec 5.4).
/// The HermiT differential is spec 5.3, plan phase 7, and is NOT
/// built, so no code path can produce a 5 today.
///
/// This asserts that absence rather than pretending to a proof, and it
/// is written to BREAK when phase 7 lands: the moment `main.rs` learns
/// to emit 5, this test fails and whoever built the differential has
/// to come here and replace it with a real observation. A permanent
/// no-op would be the very defect this file exists to close.
#[test]
fn exit_five_is_not_yet_reachable() {
    // Structural, not lexical. `main` returns through
    // `ExitCode::from(u8::try_from(run_exit_code(..)))`, so a future
    // `Verdict` arm mapping to 5 would make exit 5 fully reachable
    // while a grep for the literal `ExitCode::from(5)` stayed green.
    // Enumerate the verdicts instead and assert the mapping itself
    // cannot yield 5.
    let verdicts = [
        Verdict::Pass,
        Verdict::UnrefutedPass,
        Verdict::Fail("x".into()),
        Verdict::Indeterminate(IndeterminateReason::Timeout),
    ];
    for v in &verdicts {
        for allow in [false, true] {
            assert_ne!(
                run_exit_code(v, allow),
                5,
                "a verdict now maps to exit 5, so oracle divergence is reachable. The \
                 HermiT differential (spec 5.3, phase 7) has landed: replace this test \
                 with a real observation of divergence from the binary."
            );
        }
        assert_ne!(exit_code(v), 5, "exit_code must not yield 5 either");
    }

    // Belt: no literal 5 anywhere in src/, which would be a path that
    // bypasses the mapping entirely.
    for entry in std::fs::read_dir("src").expect("src/ should be readable") {
        let path = entry.expect("dir entry should be readable").path();
        if path.extension().is_some_and(|e| e == "rs") {
            let text = std::fs::read_to_string(&path).expect("source should be readable");
            assert!(
                !text.contains("ExitCode::from(5)"),
                "{} can now emit exit 5 directly. See the message above.",
                path.display()
            );
        }
    }
}
