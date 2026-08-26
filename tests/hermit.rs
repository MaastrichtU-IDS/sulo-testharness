//! The ROBOT/HermiT driver, against a real jar.
//!
//! # Why these tests are gated, and why the gate itself is tested
//!
//! The differential is a CI job: the JVM stays off the default and
//! local path (spec 5.3), so `cargo test` on a laptop with no Java
//! must still pass. Every test below therefore skips, loudly and by
//! name, unless `SULO_ROBOT_JAR` points at a readable ROBOT jar.
//!
//! A skip-when-unset gate is one wrong line away from being a suite
//! that can never fail, so the gate is a pure function
//! (`hermit::resolve_jar`) with its own tests, and it distinguishes
//! three states rather than two:
//!
//! * unset: skip, with a message naming the variable.
//! * set to something unusable (empty, or not a readable file): PANIC.
//!   A CI job that thought it had configured the jar and quietly ran
//!   zero assertions is precisely the "green while testing nothing"
//!   outcome this repository keeps finding, so a misconfigured
//!   variable is louder than an unset one, not quieter.
//! * set to a readable file: run.
//!
//! And a fourth state on top of those three, ruling 9: with
//! `SULO_ROBOT_JAR_REQUIRED` set, an unset `SULO_ROBOT_JAR` stops
//! being a skip and becomes a failure. The differential CI job sets
//! it, because there a typo in the step that exports the jar path
//! would otherwise leave every test below skipping and the job green.
//!
//! The gate lives in `src/hermit.rs` rather than here because three
//! test binaries share it (`tests/hermit.rs`, `tests/differential.rs`,
//! `tests/cli.rs`), and three copies of a skip rule is three chances
//! for one of them to skip when it should fail.
//!
//! The classification arms (which message means what) are unit-tested
//! without a JVM in `src/hermit.rs`. What can only be tested here is
//! that the real ROBOT, driven by this code, gives the four answers
//! the plan measured.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use sulo_testharness::hermit::{
    HermitAnswer, JAR_REQUIRED_VAR, JAR_VAR, Jar, consistency, consistency_with_deadline,
    jar_from_env, resolve_jar,
};

/// Real SULO, the ontology every differential question is really
/// about.
const SULO: &str = "../sulo/sulo.ttl";

/// The jar, or `None` after saying why. Thin wrapper so a reader of a
/// test below sees the gate by name.
fn jar() -> Option<PathBuf> {
    jar_from_env()
}

/// A per-test scratch directory, removed first so a previous run's
/// merged.ttl can never be the thing an assertion passes on.
fn workdir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "sulo-testharness-hermit-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn sulo() -> &'static Path {
    let p = Path::new(SULO);
    assert!(
        p.is_file(),
        "{SULO} must exist: these tests are about real SULO, and a missing checkout \
         must fail rather than quietly test a fixture"
    );
    p
}

// ---------------------------------------------------------------
// The gate.
// ---------------------------------------------------------------

#[test]
fn an_unset_variable_skips() {
    assert_eq!(resolve_jar(None, None), Jar::Skip);
}

#[test]
fn an_empty_variable_is_a_misconfiguration_not_a_skip() {
    match resolve_jar(Some(OsString::from("")), None) {
        Jar::Misconfigured(msg) => assert!(msg.contains("set but empty"), "{msg}"),
        other => panic!("an empty {JAR_VAR} must be refused, got {other:?}"),
    }
}

#[test]
fn a_variable_pointing_at_nothing_is_a_misconfiguration_not_a_skip() {
    match resolve_jar(Some(OsString::from("/nonexistent/robot.jar")), None) {
        Jar::Misconfigured(msg) => assert!(msg.contains("not a readable file"), "{msg}"),
        other => panic!("an unusable {JAR_VAR} must be refused, got {other:?}"),
    }
}

#[test]
fn a_variable_pointing_at_a_file_is_used() {
    let existing = OsString::from("tests/fixtures/clean.ttl");
    assert_eq!(
        resolve_jar(Some(existing), None),
        Jar::Use(PathBuf::from("tests/fixtures/clean.ttl"))
    );
}

// ---------------------------------------------------------------
// Ruling 9: strict mode, where a skip is a failure.
// ---------------------------------------------------------------

/// The row the differential CI job depends on. Without it, a typo in
/// the workflow step that exports the jar path leaves every jar-gated
/// test in this repository skipping, and the job reports a confident
/// green having asserted nothing about either reasoner.
#[test]
fn strict_mode_turns_a_missing_jar_into_a_failure_not_a_skip() {
    match resolve_jar(None, Some(OsString::from("1"))) {
        Jar::Misconfigured(msg) => {
            assert!(
                msg.contains(JAR_REQUIRED_VAR) && msg.contains("a skip is a failure"),
                "the refusal must name the variable that caused it: {msg}"
            );
        }
        other => panic!(
            "with {JAR_REQUIRED_VAR} set, an unset {JAR_VAR} must be refused, got \
             {other:?}. A skip here is a green CI job that asserted nothing"
        ),
    }
}

/// `"0"` and `"false"` turn strict mode ON, like every other non-empty
/// value. Pinned because it looks wrong at a glance and is deliberate:
/// guessing that `"0"` means "not strict" makes a workflow typo
/// silently green, while guessing the other way can only ever produce
/// a loud failure a human then looks at.
#[test]
fn any_non_empty_value_turns_strict_mode_on() {
    for value in ["0", "false", "no", "1"] {
        assert!(
            matches!(
                resolve_jar(None, Some(OsString::from(value))),
                Jar::Misconfigured(_)
            ),
            "{JAR_REQUIRED_VAR}={value:?} must be strict"
        );
    }
}

/// An EMPTY `SULO_ROBOT_JAR_REQUIRED` is not strict, so a workflow
/// that writes `${{ env.MAYBE }}` into it and gets an empty string
/// still skips rather than failing for a reason nobody asked for.
#[test]
fn an_empty_required_variable_is_not_strict() {
    assert_eq!(resolve_jar(None, Some(OsString::from(""))), Jar::Skip);
}

/// Strict mode does not invent a jar: a jar that IS set and IS usable
/// is still used, and strictness changes nothing about it.
#[test]
fn strict_mode_still_uses_a_usable_jar() {
    assert_eq!(
        resolve_jar(
            Some(OsString::from("tests/fixtures/clean.ttl")),
            Some(OsString::from("1"))
        ),
        Jar::Use(PathBuf::from("tests/fixtures/clean.ttl"))
    );
}

// ---------------------------------------------------------------
// The four answers, from the real jar.
// ---------------------------------------------------------------

/// A consistent ontology. Deliberately first: without this, every
/// other assertion below could be satisfied by a driver that answered
/// `Inconsistent` or `Error` unconditionally.
#[test]
fn a_consistent_ontology_is_consistent() {
    let Some(robot) = jar() else { return };
    let answer = consistency(
        &robot,
        Path::new("tests/fixtures/clean.ttl"),
        &[],
        &workdir("clean"),
    );
    assert_eq!(answer, HermitAnswer::Consistent);
}

/// Real SULO on its own. The differential's baseline: if this were
/// ever anything but `Consistent`, every probe question built on top
/// of it would be inconsistent for a reason that has nothing to do
/// with the question.
#[test]
fn real_sulo_is_consistent() {
    let Some(robot) = jar() else { return };
    let answer = consistency(&robot, sulo(), &[], &workdir("sulo"));
    assert_eq!(answer, HermitAnswer::Consistent);
}

/// A flat, fixture-level clash.
#[test]
fn a_known_clash_is_inconsistent() {
    let Some(robot) = jar() else { return };
    let answer = consistency(
        &robot,
        Path::new("tests/fixtures/inconsistent.ttl"),
        &[],
        &workdir("clash"),
    );
    assert_eq!(answer, HermitAnswer::Inconsistent);
}

/// A clash that only SULO's own `Object owl:disjointWith Process`
/// produces. Proves HermiT is reasoning over the merged ontology,
/// not just parsing the data file.
#[test]
fn sulos_own_disjointness_is_inconsistent() {
    let Some(robot) = jar() else { return };
    let answer = consistency(
        &robot,
        sulo(),
        &[PathBuf::from(
            "tests/fixtures/sulo-object-process-clash.ttl",
        )],
        &workdir("disjoint"),
    );
    assert_eq!(answer, HermitAnswer::Inconsistent);
}

/// Measured result 3, and the reason this module exists: rustdl
/// reports this case consistent because horned-owl drops the
/// data-range `allValuesFrom` axiom as unsupported, so the harness
/// defers it (`oracle-hermit`). HermiT decides it.
///
/// It is also the case that fails if anyone rewrites the driver into
/// the chained single-process form, which measures as CONSISTENT
/// here.
#[test]
fn the_data_range_case_is_inconsistent() {
    let Some(robot) = jar() else { return };
    let answer = consistency(
        &robot,
        sulo(),
        &[PathBuf::from(
            "suites/sulo/restrictions/data/timeinstant-datarange.ttl",
        )],
        &workdir("datarange"),
    );
    assert_eq!(
        answer,
        HermitAnswer::Inconsistent,
        "HermiT must decide the case rustdl provably cannot; a CONSISTENT here is the \
         signature of the chained merge+reason invocation"
    );
}

/// The important one. ROBOT exits 1 both for a real inconsistency and
/// for a bad invocation, so a driver that read the exit code would
/// report this missing file as a detected clash, which under the
/// non-entailment encoding reads as "entailed". Both `Consistent` and
/// `Inconsistent` are wrong answers here.
#[test]
fn an_induced_robot_error_is_an_error_not_a_verdict() {
    let Some(robot) = jar() else { return };
    let answer = consistency(
        &robot,
        Path::new("tests/fixtures/no-such-ontology.ttl"),
        &[],
        &workdir("error"),
    );
    match answer {
        HermitAnswer::Error(msg) => {
            assert!(
                msg.contains("no-such-ontology.ttl"),
                "the error must name what went wrong: {msg}"
            );
            assert!(
                msg.contains("nothing was learned"),
                "the error must say it is not a verdict: {msg}"
            );
        }
        other => panic!(
            "a missing input must be Error. Getting {other:?} means an invocation \
             failure is being reported as a reasoner result"
        ),
    }
}

/// The second step's error arm, which the missing-input test above
/// cannot reach: this ontology merges cleanly and is then REFUSED by
/// the reasoner (a non-simple property in a cardinality restriction is
/// outside OWL 2 DL), so `robot reason` exits 1 with a message that is
/// not an inconsistency report. Reading that exit 1 as a verdict would
/// report a clash nobody found, and under the non-entailment encoding
/// a clash reads as "entailed".
#[test]
fn a_reason_step_failure_is_an_error_not_an_inconsistency() {
    let Some(robot) = jar() else { return };
    let answer = consistency(
        &robot,
        Path::new("tests/fixtures/outside-owl2-dl.ttl"),
        &[],
        &workdir("nonsimple"),
    );
    match answer {
        HermitAnswer::Error(msg) => {
            assert!(
                msg.contains("Non-simple property"),
                "the error must quote what ROBOT said: {msg}"
            );
            assert!(
                msg.contains("not a verdict"),
                "the error must say it is not a verdict: {msg}"
            );
        }
        other => panic!(
            "a reasoner that refused the ontology must be Error, got {other:?}: the \
             `reason` step exits 1 for a bad invocation exactly as it does for a real \
             inconsistency"
        ),
    }
}

/// A deadline that cannot be met is an `Error`. One millisecond is
/// not enough to start a JVM, let alone merge an ontology, so this is
/// deterministic; what is being pinned is that expiry produces
/// `Error` and not the `Consistent` that would silently agree with
/// rustdl on every negative question.
#[test]
fn an_exceeded_deadline_is_an_error() {
    let Some(robot) = jar() else { return };
    let answer = consistency_with_deadline(
        &robot,
        sulo(),
        &[],
        &workdir("deadline"),
        Duration::from_millis(1),
    );
    match answer {
        HermitAnswer::Error(msg) => assert!(
            msg.contains("deadline"),
            "the error must say the deadline expired: {msg}"
        ),
        other => panic!("an exceeded deadline must be Error, got {other:?}"),
    }
}
