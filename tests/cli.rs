//! Every documented exit code, observed from the actual binary.
//!
//! Spec 5.4 has defined codes 0 to 5 since the design was written, and
//! `tests/verdict.rs` has pinned `verdict::exit_code` since the engine
//! plan. Neither proved the PROGRAM could produce them: before the
//! `run` subcommand existed, codes 1 and 3 were unreachable from the
//! binary, and before the `differential` subcommand existed so was 5.
//! A contract nothing can exercise is this project's recurring defect
//! shape (a check that cannot fail) wearing the clothes of a
//! documented interface. All six codes are now observed here.
//!
//! So every row here launches `CARGO_BIN_EXE_sulo-testharness` and
//! asserts the observed status. A unit test over the mapping function
//! is not a substitute: it cannot catch a `main` that forgets to
//! propagate, that aggregates the wrong set, or that prints a report
//! and returns success anyway.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use sulo_testharness::hermit::jar_from_env;

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
// 5: oracle divergence.
// ---------------------------------------------------------------
//
// Until the HermiT differential landed, this section asserted the
// ABSENCE of exit 5 and was written to break when the differential
// arrived. It did not break, which is worth recording: that test
// enumerated `Verdict` against `verdict::exit_code` and grepped `src/`
// for the literal `ExitCode::from(5)`, and `main` reaches 5 through
// neither (`differential_exit_code` is its own mapping, over
// comparisons rather than verdicts). A test written to fail when a
// feature lands only fails if it watches the route the feature
// actually takes.
//
// So the rows below are real observations instead. BOTH directions,
// because a differential that has never been seen to diverge is not
// evidence of agreement, and one that has never been seen to agree is
// not evidence that it can do anything but complain. Ruling 4.

/// The jar, or `None` after saying why. Same three-state gate every
/// other jar-dependent test in this repository uses; `SULO_ROBOT_JAR`
/// unset is a skip on a laptop, and a failure in the differential CI
/// job, which sets `SULO_ROBOT_JAR_REQUIRED`.
fn robot_jar() -> Option<PathBuf> {
    jar_from_env()
}

fn differential(jar: &Path, filter: &str, scratch_name: &str) -> Output {
    let workdir = scratch(scratch_name);
    run(&[
        "differential",
        "--suite",
        SUITE,
        "--ontology",
        SULO,
        "--robot",
        jar.to_str().expect("the jar path is UTF-8"),
        "--filter",
        filter,
        "--workdir",
        workdir.to_str().expect("the scratch path is UTF-8"),
    ])
}

/// Exit 5, observed from the binary, on the one case where the two
/// reasoners really do disagree.
///
/// `timeinstant-datarange` asserts a data-range `allValuesFrom` the
/// pinned rustdl build cannot represent at all, so rustdl reports the
/// ontology plus the offending data CONSISTENT while HermiT finds the
/// clash. This is the case the differential was built for, and it is
/// the only proof that the whole path (question building, probe,
/// ROBOT, classification, comparison, exit code) can report a
/// disagreement rather than manufacture agreement.
#[test]
fn a_diverging_case_exits_five() {
    require_sulo();
    let Some(jar) = robot_jar() else { return };

    let out = differential(&jar, "restrictions/timeinstant-datarange", "diverge");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        out.status.code(),
        Some(5),
        "a genuine disagreement between the two reasoners must exit 5. stdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        stdout.contains("DIVERGENCE") && stdout.contains("timeinstant-datarange"),
        "the divergence must be NAMED, not just counted in an exit code:\n{stdout}"
    );
    assert!(
        stdout.contains("rustdl: consistent") && stdout.contains("HermiT: inconsistent"),
        "BOTH answers must be reported: the reader's job is to work out which reasoner \
         is wrong, and neither answer alone lets them:\n{stdout}"
    );
    assert!(
        stdout.contains("rustdl is the outlier"),
        "the report must say which reasoner is the outlier:\n{stdout}"
    );
}

/// The other direction, and it is not optional: a differential that
/// answered `Divergence` to everything would pass the test above while
/// being worth nothing.
///
/// `non-subsumptions` puts five questions (its consistency gate and
/// four non-subsumptions) to both reasoners, and every one of them
/// comes back the same from each.
#[test]
fn an_agreeing_case_exits_zero() {
    require_sulo();
    let Some(jar) = robot_jar() else { return };

    let out = differential(&jar, "taxonomy/non-subsumptions", "agree");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        out.status.code(),
        Some(0),
        "two reasoners giving the same answer to every question must exit 0. stdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        stdout.contains("5 question(s): 5 agreed, 0 diverged, 0 indeterminate"),
        "the run must report that it actually ASKED five questions; a green summary over \
         zero questions is the failure this subcommand's own guards exist to \
         prevent:\n{stdout}"
    );
}

/// A configuration error is exit 2 here exactly as it is on `run`, and
/// is never a statement about either reasoner. Needs no jar, which is
/// the point: this arm is reached before any case is loaded.
#[test]
fn a_differential_with_an_unusable_jar_exits_two() {
    require_sulo();
    let out = run(&[
        "differential",
        "--suite",
        SUITE,
        "--ontology",
        SULO,
        "--robot",
        "/nonexistent/robot.jar",
        "--filter",
        "taxonomy/deep-chain",
    ]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(2),
        "an unusable --robot is a configuration error, not a divergence and not an \
         agreement. stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("--robot") && stderr.contains("not a readable file"),
        "the refusal must name the flag that was wrong:\n{stderr}"
    );
}

// ---------------------------------------------------------------
// 5b: the pinned set of KNOWN divergences (ruling 12).
// ---------------------------------------------------------------
//
// `tests/divergences.rs` exercises the diff itself over synthetic
// runs, with no JVM anywhere. What only the binary can show is that
// `--divergences` actually reaches the process exit code, in all
// three directions: matched is 0, an unpinned divergence is 5, and a
// pinned divergence that no longer occurs is 4.
//
// These run over `suites/sulo/restrictions` rather than the whole
// suite, because the whole suite is 92 questions and about two and a
// half minutes of JVM. The sub-suite holds the one case the two
// reasoners disagree about, which is all these rows need. Its pin is
// written into a scratch directory, so `suites/sulo.divergences`
// itself is never touched by a test.

/// The sub-suite these rows run over. Its `# suite:` header must
/// match, since a pin describes one corpus and comparing it against
/// another is refused.
const RESTRICTIONS: &str = "suites/sulo/restrictions";

/// A sub-suite the two reasoners AGREE about, end to end. Needed by
/// the stale-pin rows: ruling 13 puts a live divergence above a stale
/// pin, so a run that still diverges can no longer be used to observe
/// exit 4 on its own.
const PROPERTIES: &str = "suites/sulo/properties";

/// Write a pin file for `RESTRICTIONS` holding exactly `rows`.
///
/// The reasoner version comes from the same constant the harness
/// writes, so a version bump does not turn these rows into a
/// re-baseline prompt.
fn write_pin(dir: &Path, name: &str, rows: &[&str]) -> PathBuf {
    write_pin_for(RESTRICTIONS, dir, name, rows)
}

fn write_pin_for(suite: &str, dir: &Path, name: &str, rows: &[&str]) -> PathBuf {
    let path = dir.join(name);
    let mut text = format!(
        "# suite: {suite}\n# reasoner: {}\n",
        sulo_testharness::golden::REASONER_VERSION
    );
    for row in rows {
        text.push_str(row);
        text.push('\n');
    }
    std::fs::write(&path, text).expect("the scratch pin should be writable");
    path
}

/// The one divergence the real suite produces, as a pin row.
const REAL_ROW: &str =
    "timeinstant-datarange\tgate: expected inconsistent\tgate\tconsistent\tinconsistent";

fn pinned_differential(jar: &Path, pin: &Path, scratch_name: &str) -> Output {
    pinned_differential_over(RESTRICTIONS, jar, pin, scratch_name)
}

fn pinned_differential_over(suite: &str, jar: &Path, pin: &Path, scratch_name: &str) -> Output {
    let workdir = scratch(scratch_name);
    run(&[
        "differential",
        "--suite",
        suite,
        "--ontology",
        SULO,
        "--robot",
        jar.to_str().expect("the jar path is UTF-8"),
        "--workdir",
        workdir.to_str().expect("the scratch path is UTF-8"),
        "--divergences",
        pin.to_str().expect("the pin path is UTF-8"),
    ])
}

/// A run whose divergences match the pin is GREEN.
///
/// This is the row that makes the weekly job usable at all. A job that
/// is permanently red gets muted, and a muted alarm is this project's
/// recurring defect shape wearing different clothes.
#[test]
fn a_run_matching_its_pin_exits_zero() {
    require_sulo();
    let Some(jar) = robot_jar() else { return };

    let dir = scratch("pin-match");
    let pin = write_pin(&dir, "match.divergences", &[REAL_ROW]);
    let out = pinned_differential(&jar, &pin, "pin-match-probes");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        out.status.code(),
        Some(0),
        "a divergence the pin describes is documented, not news. stdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        stdout.contains("1 as pinned, 0 unpinned, 0 stale, 0 unconfirmed"),
        "the run must say the pin was actually CONFIRMED, not merely that it exited \
         0:\n{stdout}"
    );
}

/// An unpinned divergence is exit 5. Something changed.
#[test]
fn a_divergence_outside_the_pin_exits_five() {
    require_sulo();
    let Some(jar) = robot_jar() else { return };

    let dir = scratch("pin-unpinned");
    let pin = write_pin(&dir, "empty.divergences", &[]);
    let out = pinned_differential(&jar, &pin, "pin-unpinned-probes");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        out.status.code(),
        Some(5),
        "a disagreement nobody reviewed must still be exit 5. stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("UNPINNED") && stdout.contains("timeinstant-datarange"),
        "the unpinned divergence must be NAMED:\n{stdout}"
    );
}

/// The direction ruling 12 exists for: a pinned divergence that no
/// longer occurs is exit 4, never a quiet pass.
///
/// Proved by pinning a divergence that does not happen. `duration-
/// nonnegative` is a real case in this sub-suite and the two reasoners
/// agree about it, so this row is exactly the shape of "the gap
/// closed": the pin describes something the run did not find.
#[test]
fn a_pinned_divergence_that_no_longer_occurs_exits_four() {
    require_sulo();
    let Some(jar) = robot_jar() else { return };

    let dir = scratch("pin-stale");
    let pin = write_pin(
        &dir,
        "stale.divergences",
        &[
            "duration-nonnegative\tgate: expected consistent\tgate\tconsistent\tinconsistent",
            REAL_ROW,
        ],
    );
    let out = pinned_differential(&jar, &pin, "pin-stale-probes");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        out.status.code(),
        Some(4),
        "a documented divergence that stopped occurring means the pin is a lie and must \
         be re-baselined deliberately; it is never a quiet pass. stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("STALE") && stdout.contains("duration-nonnegative"),
        "the stale entry must be NAMED:\n{stdout}"
    );
}

/// `--divergences` with `--filter` is refused: a pin is a claim about
/// a whole corpus, and a filtered run never asks the questions outside
/// the filter, so it can neither confirm nor refute those entries.
///
/// Needs no jar, because the refusal comes before any case is loaded.
#[test]
fn a_pinned_run_cannot_be_narrowed_by_a_filter() {
    require_sulo();
    let out = run(&[
        "differential",
        "--suite",
        SUITE,
        "--ontology",
        SULO,
        "--robot",
        "tests/fixtures/clean.ttl",
        "--divergences",
        "suites/sulo.divergences",
        "--filter",
        "restrictions",
    ]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(2),
        "a filtered run cannot judge a whole-suite pin. stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("--divergences") && stderr.contains("--filter"),
        "the refusal must name both flags:\n{stderr}"
    );
}

/// `--accept-divergences` without `--divergences` writes nothing, so
/// it is refused rather than ignored: silently accepting a no-op flag
/// teaches an operator that they re-baselined when they did not.
#[test]
fn accepting_a_pin_that_was_never_named_is_refused() {
    require_sulo();
    let out = run(&[
        "differential",
        "--suite",
        SUITE,
        "--ontology",
        SULO,
        "--robot",
        "tests/fixtures/clean.ttl",
        "--accept-divergences",
    ]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "stderr:\n{stderr}");
    assert!(
        stderr.contains("--accept-divergences") && stderr.contains("--divergences"),
        "the refusal must name both flags:\n{stderr}"
    );
}

/// A pin recorded against a different reasoner is exit 4 from the
/// binary, not a comparison, AND the questions are still reported.
///
/// Covers `main`'s routing of `PinOutcome::RebaselineRequired`, which
/// no unit test can reach: `check_pin` returning the variant and the
/// process returning 4 are two different claims, and only this one
/// observes the second.
///
/// Run over `PROPERTIES`, where the two reasoners agree about
/// everything, so 4 is the whole story. Over a sub-suite that still
/// diverges the answer is 5 (ruling 13), which the row below observes.
///
/// The report assertion is not decoration. This route used to return 4
/// BEFORE rendering anything, so a run holding a live disagreement
/// printed one line about the pin and never named the disagreement:
/// the pin's staleness outranking the news the job exists to deliver.
#[test]
fn a_pin_from_another_reasoner_exits_four_from_the_binary() {
    require_sulo();
    let Some(jar) = robot_jar() else { return };

    let dir = scratch("pin-version");
    let path = dir.join("old.divergences");
    // The pin carries a ROW, and deliberately. A stale header
    // suppresses comparison of the pin's CONTENTS, not just its
    // header, and an empty pin cannot show that: there would be
    // nothing whose comparison could have been suppressed. This row
    // names a case that does not exist in `PROPERTIES`, so a run that
    // DID compare would have to report it STALE by name. The
    // assertions below observe that neither happened.
    std::fs::write(
        &path,
        format!("# suite: {PROPERTIES}\n# reasoner: rustdl v0.0.1-ancient\n{REAL_ROW}\n"),
    )
    .expect("the scratch pin should be writable");

    let out = pinned_differential_over(PROPERTIES, &jar, &path, "pin-version-probes");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        out.status.code(),
        Some(4),
        "a pin whose provenance does not match this build is reviewed, not compared. \
         stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("re-baseline required") && stdout.contains("v0.0.1-ancient"),
        "the message must name what was stale:\n{stdout}"
    );
    assert!(
        !stdout.contains("STALE") && !stdout.contains("timeinstant-datarange"),
        "the pin's own row must be neither confirmed nor called stale: a pin this build \
         will not trust is not compared at all, so nothing in it may be reported \
         on:\n{stdout}"
    );
    assert!(
        !stdout.contains("as pinned"),
        "no pin diff may be rendered on this route; the report is printed \
         UNPINNED:\n{stdout}"
    );
    assert!(
        stdout.contains("differential: rustdl vs HermiT") && stdout.contains("question(s):"),
        "an uncomparable pin is a statement about the PIN; the questions were still asked \
         and must still be reported:\n{stdout}"
    );
}

/// `--format json` on the stale-pin route still emits ONE parseable
/// JSON document.
///
/// The CI job runs the differential twice, text and JSON, and uploads
/// both. Rendering the report on this route (see the row above) means
/// stdout now carries a JSON payload here, so the human-readable
/// "re-baseline required" line has to go somewhere that is not the
/// middle of that document.
#[test]
fn a_stale_pin_in_json_format_still_emits_parseable_json() {
    require_sulo();
    let Some(jar) = robot_jar() else { return };

    let dir = scratch("pin-version-json");
    let path = dir.join("old.divergences");
    std::fs::write(
        &path,
        format!("# suite: {PROPERTIES}\n# reasoner: rustdl v0.0.1-ancient\n"),
    )
    .expect("the scratch pin should be writable");

    let workdir = scratch("pin-version-json-probes");
    let out = run(&[
        "differential",
        "--suite",
        PROPERTIES,
        "--ontology",
        SULO,
        "--robot",
        jar.to_str().expect("the jar path is UTF-8"),
        "--workdir",
        workdir.to_str().expect("the scratch path is UTF-8"),
        "--divergences",
        path.to_str().expect("the pin path is UTF-8"),
        "--format",
        "json",
    ]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(4), "stdout:\n{stdout}");
    let payload: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("stdout must be one JSON document: {e}\n{stdout}"));
    assert_eq!(
        payload["summary"]["exit_code"], 0,
        "the payload reports what the QUESTIONS say; the 4 comes from the pin, which \
         could not be compared against and so has no diff in the payload:\n{stdout}"
    );
    assert!(
        stderr.contains("re-baseline required") && stderr.contains("v0.0.1-ancient"),
        "the pin message must still be reported, on stderr:\n{stderr}"
    );
}

/// Under `--format json`, EVERY route writes one JSON document to
/// stdout, including the routes that render no report at all.
///
/// This row is the configuration-error one, which needs no jar: the
/// refusal comes before any case is loaded. It used to leave stdout at
/// 0 bytes while the only explanation went to stderr, and the CI step
/// captures stdout twice (`> differential.json` and `| tee
/// differential.txt`), so both uploaded artifacts came back blank on
/// exactly the run that failed.
///
/// `outcome` is the discriminator, and the payload carries NO
/// `summary` key: a consumer reading `.summary.questions` off a
/// non-report must get `null`, not a `0` it could read as "every
/// question agreed".
#[test]
fn a_config_error_in_json_format_still_emits_parseable_json() {
    require_sulo();
    let out = run(&[
        "differential",
        "--suite",
        SUITE,
        "--ontology",
        SULO,
        "--robot",
        "tests/fixtures/clean.ttl",
        "--divergences",
        "suites/sulo.divergences",
        "--filter",
        "restrictions",
        "--format",
        "json",
    ]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "stderr:\n{stderr}");
    assert!(
        !stdout.trim().is_empty(),
        "stdout must not be empty: the CI step uploads it as the run's only \
         machine-readable artifact"
    );
    let payload: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("stdout must be one JSON document: {e}\n{stdout}"));
    assert_eq!(payload["outcome"], "config_error", "{stdout}");
    assert_eq!(payload["exit_code"], 2, "{stdout}");
    assert!(
        payload["error"]
            .as_str()
            .is_some_and(|e| e.contains("--divergences") && e.contains("--filter")),
        "the payload must carry the same refusal stderr got:\n{stdout}"
    );
    assert!(
        payload.get("summary").is_none() && payload.get("questions").is_none(),
        "a non-report must not be shaped like a report: nothing was asked, and a \
         `summary` of zero questions reads as agreement:\n{stdout}"
    );
    assert!(
        stderr.contains("--divergences"),
        "the human message stays on stderr too:\n{stderr}"
    );
}

/// The same rule on the two pin routes, which need a real run behind
/// them and so need the jar.
///
/// `PinOutcome::Error` (here, a pin file that does not exist) used to
/// leave stdout at 0 bytes, and `PinOutcome::Rebaselined` used to
/// print a line of plain text into what is supposed to be a JSON
/// document.
#[test]
fn the_pin_routes_in_json_format_still_emit_parseable_json() {
    require_sulo();
    let Some(jar) = robot_jar() else { return };

    let dir = scratch("pin-json-routes");
    let missing = dir.join("does-not-exist.divergences");
    let workdir = scratch("pin-json-routes-probes");
    let json_run = |pin: &Path, accept: bool| {
        let mut args = vec![
            "differential",
            "--suite",
            PROPERTIES,
            "--ontology",
            SULO,
            "--robot",
            jar.to_str().expect("the jar path is UTF-8"),
            "--workdir",
            workdir.to_str().expect("the scratch path is UTF-8"),
            "--divergences",
            pin.to_str().expect("the pin path is UTF-8"),
            "--format",
            "json",
        ];
        if accept {
            args.push("--accept-divergences");
        }
        run(&args)
    };

    let out = json_run(&missing, false);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        out.status.code(),
        Some(2),
        "a missing pin is a configuration error, never an empty one. stdout:\n{stdout}"
    );
    let payload: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("stdout must be one JSON document: {e}\n{stdout}"));
    assert_eq!(payload["outcome"], "pin_error", "{stdout}");
    assert_eq!(payload["exit_code"], 2, "{stdout}");
    assert!(
        payload.get("summary").is_none(),
        "a non-report must not be shaped like a report:\n{stdout}"
    );

    // ...and the re-baseline, which writes the file it names.
    let fresh = dir.join("fresh.divergences");
    let out = json_run(&fresh, true);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "stdout:\n{stdout}");
    let payload: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("stdout must be one JSON document: {e}\n{stdout}"));
    assert_eq!(payload["outcome"], "rebaselined", "{stdout}");
    assert_eq!(
        payload["divergences"],
        serde_json::Value::from(fresh.display().to_string()),
        "the payload must name the file that was written:\n{stdout}"
    );
    assert!(
        fresh.is_file(),
        "the re-baseline must actually have written {}",
        fresh.display()
    );
    assert!(
        stderr.contains("pinned divergences written to"),
        "the human line moves to stderr under --format json, it is not dropped:\n{stderr}"
    );
}

/// Ruling 13's precedence, in the one place it was inverted: 5 over 4.
///
/// A stale-provenance pin over a sub-suite that genuinely diverges is
/// not a 4. The pin cannot be compared against, which is news about
/// the pin; the two reasoners disagreeing about `timeinstant-
/// datarange` is news about SULO and rustdl, and ruling 13 ranks the
/// live disagreement higher. A reader handed a 4 here would go and fix
/// the pin without ever learning that anything diverged.
#[test]
fn a_live_divergence_outranks_a_stale_pin_and_is_still_named() {
    require_sulo();
    let Some(jar) = robot_jar() else { return };

    let dir = scratch("pin-stale-and-diverging");
    let path = dir.join("ancient.divergences");
    std::fs::write(
        &path,
        format!("# suite: {RESTRICTIONS}\n# reasoner: rustdl v0.0.1-ancient\n"),
    )
    .expect("the scratch pin should be writable");

    let out = pinned_differential(&jar, &path, "pin-stale-and-diverging-probes");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        out.status.code(),
        Some(5),
        "ruling 13: a live disagreement outranks an uncomparable pin. stdout:\n{stdout}\n\
         stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        stdout.contains("DIVERGENCE") && stdout.contains("timeinstant-datarange"),
        "the disagreement must be NAMED, not swallowed by the pin's staleness:\n{stdout}"
    );
    assert!(
        stdout.contains("re-baseline required") && stdout.contains("v0.0.1-ancient"),
        "the pin problem must be reported too; the exit code just does not belong to \
         it:\n{stdout}"
    );
}
