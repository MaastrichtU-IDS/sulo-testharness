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
//! (`resolve_jar`) with its own tests, and it distinguishes three
//! states rather than two:
//!
//! * unset: skip, with a message naming the variable.
//! * set to something unusable (empty, or not a readable file): PANIC.
//!   A CI job that thought it had configured the jar and quietly ran
//!   zero assertions is precisely the "green while testing nothing"
//!   outcome this repository keeps finding, so a misconfigured
//!   variable is louder than an unset one, not quieter.
//! * set to a readable file: run.
//!
//! The classification arms (which message means what) are unit-tested
//! without a JVM in `src/hermit.rs`. What can only be tested here is
//! that the real ROBOT, driven by this code, gives the four answers
//! the plan measured.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use sulo_testharness::hermit::{HermitAnswer, consistency, consistency_with_deadline};

/// The environment variable naming the ROBOT jar.
const JAR_VAR: &str = "SULO_ROBOT_JAR";

/// Real SULO, the ontology every differential question is really
/// about.
const SULO: &str = "../sulo/sulo.ttl";

/// What the gate decided about `SULO_ROBOT_JAR`.
#[derive(Debug, PartialEq, Eq)]
enum Jar {
    Use(PathBuf),
    Skip,
    Misconfigured(String),
}

/// Decide from the variable's value alone, so the decision can be
/// tested without mutating the process environment (which is `unsafe`
/// in edition 2024 and racy under a parallel test binary anyway).
fn resolve_jar(value: Option<OsString>) -> Jar {
    match value {
        None => Jar::Skip,
        Some(v) if v.is_empty() => Jar::Misconfigured(format!(
            "{JAR_VAR} is set but empty. That is a broken configuration, not a request \
             to skip: unset it to skip these tests."
        )),
        Some(v) => {
            let path = PathBuf::from(v);
            if path.is_file() {
                Jar::Use(path)
            } else {
                Jar::Misconfigured(format!(
                    "{JAR_VAR} is set to {}, which is not a readable file. Refusing to \
                     skip: a differential that silently ran nothing would report a \
                     confident green.",
                    path.display()
                ))
            }
        }
    }
}

/// The jar, or `None` after printing why the caller should stop.
fn jar() -> Option<PathBuf> {
    match resolve_jar(std::env::var_os(JAR_VAR)) {
        Jar::Use(p) => Some(p),
        Jar::Skip => {
            eprintln!(
                "SKIPPED: {JAR_VAR} is not set, so the HermiT differential driver was \
                 not exercised. Set it to a ROBOT 1.9.7 jar to run these tests."
            );
            None
        }
        Jar::Misconfigured(msg) => panic!("{msg}"),
    }
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
    assert_eq!(resolve_jar(None), Jar::Skip);
}

#[test]
fn an_empty_variable_is_a_misconfiguration_not_a_skip() {
    match resolve_jar(Some(OsString::from(""))) {
        Jar::Misconfigured(msg) => assert!(msg.contains("set but empty"), "{msg}"),
        other => panic!("an empty {JAR_VAR} must be refused, got {other:?}"),
    }
}

#[test]
fn a_variable_pointing_at_nothing_is_a_misconfiguration_not_a_skip() {
    match resolve_jar(Some(OsString::from("/nonexistent/robot.jar"))) {
        Jar::Misconfigured(msg) => assert!(msg.contains("not a readable file"), "{msg}"),
        other => panic!("an unusable {JAR_VAR} must be refused, got {other:?}"),
    }
}

#[test]
fn a_variable_pointing_at_a_file_is_used() {
    let existing = OsString::from("tests/fixtures/clean.ttl");
    assert_eq!(
        resolve_jar(Some(existing)),
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
