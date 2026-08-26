//! `suite::run_suite`, the whole-suite loop the `run` subcommand is a
//! thin shell around.
//!
//! The loop lives in the library rather than in `main.rs` precisely so
//! these tests can exist: a guard that only exists inside `main` can
//! only be tested by launching the binary, and this project has a
//! standing habit of documenting behaviour that no test reaches.
//!
//! Two properties carry the most risk and get the most attention:
//!
//! 1. **Aggregation is over every case.** Aggregating only the last
//!    one would turn a suite whose first case fails into a green
//!    build, and would go unnoticed for as long as the last case
//!    passes, which is most of the time.
//! 2. **Configuration errors abort rather than being reported as
//!    cases.** A filter that matches nothing, a manifest that will not
//!    parse, an `--ontology` that is not there: none is evidence about
//!    SULO, and none may come back as either a green run or a red
//!    case.

use std::path::{Path, PathBuf};

use sulo_testharness::suite::{RunOptions, RunOutcome, aggregate_cases};
use sulo_testharness::verdict::{Verdict, exit_code};

/// `clean.ttl` declares `ex:A`, and `ex:B rdfs:subClassOf ex:A`. So
/// `ex:B rdfs:subClassOf ex:A` is entailed (a Pass) and
/// `ex:A rdfs:subClassOf ex:B` is not (a Fail). One tiny consistent
/// ontology gives both verdicts, with no dependence on real SULO.
const CLEAN: &str = "tests/fixtures/clean.ttl";

const FAIL_CASE: &str = "\
id: fail-first
description: A subsumption clean.ttl does not entail, so this case fails.
prefixes:
  ex: http://example.org/
entails: |
  ex:A rdfs:subClassOf ex:B .
";

const PASS_CASE: &str = "\
id: pass-last
description: A subsumption clean.ttl does entail, so this case passes.
prefixes:
  ex: http://example.org/
entails: |
  ex:B rdfs:subClassOf ex:A .
";

/// An unbound prefix, which `suite::run_case` turns into
/// `Indeterminate(OracleError)`: the check could not be ASKED, so it
/// was neither passed nor failed.
///
/// `timeout_ms: 0` was tried here first and does NOT work: measured on
/// this fixture, a subsumption check with a zero deadline still comes
/// back `Pass`, because the reasoner decides it without entering the
/// tableau where the deadline is consulted. See the report on
/// `manifest::Case::timeout_ms`, whose doc comment overstated this.
const INDETERMINATE_CASE: &str = "\
id: indeterminate-middle
description: An unbound prefix, so this check cannot be asked at all.
unsatisfiable:
  - nope:Missing
";

/// A fresh scratch suite directory, unique per test and per process.
fn suite_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "sulo-testharness-run-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir should be creatable");
    dir
}

fn write_case(dir: &Path, name: &str, body: &str) {
    std::fs::write(dir.join(name), body).expect("case should be writable");
}

/// Copy the shared `clean.ttl` fixture into the scratch suite, so a
/// case can name it as its own `ontology:` relative to its base dir.
fn copy_clean(dir: &Path) -> PathBuf {
    let dest = dir.join("clean.ttl");
    std::fs::copy(CLEAN, &dest).expect("clean.ttl fixture should exist and be copyable");
    dest
}

fn run(dir: &Path, ontology: Option<&Path>, filter: Option<&str>) -> RunOutcome {
    sulo_testharness::suite::run_suite(&RunOptions {
        suite: dir,
        ontology,
        filter,
    })
}

fn ran(outcome: RunOutcome) -> Vec<sulo_testharness::suite::CaseResult> {
    match outcome {
        RunOutcome::Ran(r) => r,
        RunOutcome::Config(msg) => panic!("expected the suite to run, got a config error: {msg}"),
    }
}

fn config_error(outcome: RunOutcome) -> String {
    match outcome {
        RunOutcome::Config(msg) => msg,
        RunOutcome::Ran(r) => panic!(
            "expected a configuration error, but {} case(s) ran and were judged",
            r.len()
        ),
    }
}

// ---------------------------------------------------------------
// Aggregation is over ALL cases.
// ---------------------------------------------------------------

#[test]
fn aggregation_covers_every_case_and_not_just_the_last() {
    // Named so that discovery's sort puts the FAILING case first and
    // the PASSING case last. A run that aggregated only the final case
    // would report Pass and exit 0 here, which is the whole point of
    // the fixture.
    let dir = suite_dir("aggregate-all");
    write_case(&dir, "01-fail.yaml", FAIL_CASE);
    write_case(&dir, "02-pass.yaml", PASS_CASE);

    let results = ran(run(&dir, Some(Path::new(CLEAN)), None));

    assert_eq!(results.len(), 2, "both cases should have run");
    assert_eq!(
        results[0].id, "fail-first",
        "the failing case must sort first, or this fixture proves nothing"
    );
    assert!(
        matches!(results[0].verdict, Verdict::Fail(_)),
        "the first case should fail, got {:?}",
        results[0].verdict
    );
    assert_eq!(
        results[1].verdict,
        Verdict::Pass,
        "the LAST case should pass, so aggregating only it would hide the failure"
    );

    let overall = aggregate_cases(&results);
    assert!(
        matches!(overall, Verdict::Fail(_)),
        "one failing case anywhere must fail the run, got {overall:?}"
    );
    assert_eq!(exit_code(&overall), 1, "any Fail is exit 1 per spec 5.4");
}

#[test]
fn indeterminate_anywhere_outranks_a_pass_but_not_a_fail() {
    // The precedence Fail > Indeterminate > Pass, exercised across
    // CASES rather than across checks within one case. The middle case
    // is the one that decides, so neither "first only" nor "last only"
    // would produce these answers.
    let dir = suite_dir("aggregate-precedence");
    write_case(&dir, "01-indeterminate.yaml", INDETERMINATE_CASE);
    write_case(&dir, "02-pass.yaml", PASS_CASE);

    let results = ran(run(&dir, Some(Path::new(CLEAN)), None));
    let overall = aggregate_cases(&results);
    assert!(
        matches!(overall, Verdict::Indeterminate(_)),
        "an Indeterminate case must outrank a passing one, got {overall:?}"
    );
    assert_eq!(
        exit_code(&overall),
        3,
        "any Indeterminate is exit 3 per spec 5.4"
    );

    write_case(&dir, "03-fail.yaml", FAIL_CASE);
    let results = ran(run(&dir, Some(Path::new(CLEAN)), None));
    assert_eq!(results.len(), 3);
    let overall = aggregate_cases(&results);
    assert!(
        matches!(overall, Verdict::Fail(_)),
        "a Fail must outrank an Indeterminate, got {overall:?}"
    );
}

// ---------------------------------------------------------------
// Filtering.
// ---------------------------------------------------------------

#[test]
fn a_filter_narrowing_to_one_case_exits_on_that_cases_verdict() {
    let dir = suite_dir("filter-one");
    write_case(&dir, "01-fail.yaml", FAIL_CASE);
    write_case(&dir, "02-pass.yaml", PASS_CASE);

    // Both directions, so the test cannot pass by the filter being
    // ignored: ignoring it would run both cases and give Fail for the
    // second assertion too.
    let only_failing = ran(run(&dir, Some(Path::new(CLEAN)), Some("01-fail")));
    assert_eq!(only_failing.len(), 1, "the filter should select one case");
    assert_eq!(only_failing[0].id, "fail-first");
    assert_eq!(exit_code(&aggregate_cases(&only_failing)), 1);

    let only_passing = ran(run(&dir, Some(Path::new(CLEAN)), Some("02-pass")));
    assert_eq!(only_passing.len(), 1, "the filter should select one case");
    assert_eq!(only_passing[0].id, "pass-last");
    assert_eq!(
        exit_code(&aggregate_cases(&only_passing)),
        0,
        "a filter that excludes the failing case must exit 0 on the survivor"
    );
}

#[test]
fn a_filter_matching_nothing_is_a_configuration_error() {
    let dir = suite_dir("filter-none");
    write_case(&dir, "01-fail.yaml", FAIL_CASE);
    write_case(&dir, "02-pass.yaml", PASS_CASE);

    let msg = config_error(run(&dir, Some(Path::new(CLEAN)), Some("no-such-case")));

    assert!(
        msg.contains("no-such-case") && msg.contains("check nothing"),
        "the error must name the filter and say why matching nothing is refused, got: {msg}"
    );
}

// ---------------------------------------------------------------
// Configuration errors abort, and are never dressed up as cases.
// ---------------------------------------------------------------

#[test]
fn a_manifest_that_will_not_load_aborts_the_run() {
    // Ruling 4: a malformed manifest is not evidence about the
    // ontology, so it aborts with a configuration error rather than
    // appearing as a failing case next to the real ones. `config_error`
    // panics if any case was judged, which is the assertion.
    let dir = suite_dir("bad-manifest");
    write_case(&dir, "01-fail.yaml", FAIL_CASE);
    write_case(
        &dir,
        "02-typo.yaml",
        "id: typo-case\ndescription: Has a key the schema does not define.\nentials: |\n  ex:A rdfs:subClassOf ex:B .\n",
    );

    let msg = config_error(run(&dir, Some(Path::new(CLEAN)), None));

    assert!(
        msg.contains("02-typo.yaml"),
        "the error must name the offending manifest, got: {msg}"
    );
}

#[test]
fn an_ontology_that_is_not_a_file_is_a_configuration_error() {
    let dir = suite_dir("missing-ontology");
    write_case(&dir, "01-pass.yaml", PASS_CASE);

    let msg = config_error(run(&dir, Some(Path::new("no/such/ontology.ttl")), None));

    assert!(
        msg.contains("no/such/ontology.ttl"),
        "the error must name the ontology that is missing, got: {msg}"
    );
}

#[test]
fn a_case_needing_a_default_ontology_without_one_is_a_configuration_error() {
    // Without this guard the case would load the empty path, fail, and
    // be reported as `Indeterminate(OracleError)`: exit 3, and a
    // message about the ontology, for what is a missing command-line
    // flag.
    let dir = suite_dir("no-ontology-flag");
    write_case(&dir, "01-pass.yaml", PASS_CASE);

    let msg = config_error(run(&dir, None, None));

    assert!(
        msg.contains("pass-last") && msg.contains("--ontology"),
        "the error must name the case and the missing flag, got: {msg}"
    );
}

#[test]
fn a_case_naming_its_own_ontology_needs_no_flag() {
    // The other side of the guard above: `--ontology` is genuinely
    // optional, not merely documented as optional.
    let dir = suite_dir("own-ontology");
    copy_clean(&dir);
    write_case(
        &dir,
        "01-pass.yaml",
        &format!("ontology: clean.ttl\n{PASS_CASE}"),
    );

    let results = ran(run(&dir, None, None));

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].verdict, Verdict::Pass);
}

#[test]
fn a_discovery_error_is_a_configuration_error() {
    let dir = suite_dir("empty-suite");

    let msg = config_error(run(&dir, Some(Path::new(CLEAN)), None));

    assert!(
        msg.contains("check nothing"),
        "a suite with no cases must be refused, got: {msg}"
    );
}
