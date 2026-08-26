//! The ROBOT/HermiT driver: spawn a reasoner that is COMPLETE for
//! OWL 2 DL, and read its answer back honestly.
//!
//! Three things here are counter-intuitive enough that they were
//! measured against ROBOT 1.9.7 and OpenJDK 21 rather than assumed,
//! and each one has a guard below.
//!
//! # 1. The two-step invocation is mandatory
//!
//! Chaining the commands in one process,
//! `merge --input A --input B reason --reasoner hermit --output out.ttl`,
//! reports `suites/sulo/restrictions/data/timeinstant-datarange.ttl`
//! merged with real SULO as CONSISTENT, which is the WRONG answer.
//! Merging to a file first and then running `reason` on that file
//! reports it INCONSISTENT, which is the right one. Deterministic,
//! five runs each way, and reproduced again while writing this
//! module. A differential built the chained way would be permanently,
//! silently green: every question would come back "consistent", every
//! "consistent" reads as "not entailed", and "not entailed" is exactly
//! what rustdl already says, so the cross-check would agree with
//! itself forever while proving nothing.
//!
//! The chained form is the one a maintainer would naturally write, so
//! `two_step_plan` is a named function with a test pinning that the
//! second step's `--input` IS the first step's `--output`, and that
//! neither step's argv contains the other's command.
//!
//! # 2. The exit code is not the answer; the message is
//!
//! `reason` exits 1 when the ontology is inconsistent AND when the
//! invocation is wrong. Measured, all exiting 1 and all
//! indistinguishable by status alone:
//!
//! * inconsistent ontology: `ERROR ... The ontology is inconsistent.`
//! * `--output /dev/null`: `INVALID FORMAT ERROR unknown format: /dev/null`
//! * a missing input: `OWLOntologyInputSourceException: ... FileNotFoundException`
//!
//! So classification greps for [`INCONSISTENT_MARKER`] and treats any
//! other non-zero exit as [`HermitAnswer::Error`], never as a verdict.
//! `classify_reason` is a pure function precisely so its four arms can
//! be unit-tested here with no JVM present.
//!
//! # 3. An unanswered question is an error, never "consistent"
//!
//! Every failure route in this module (a spawn that never started, a
//! merge that failed, a deadline that expired, a process killed by a
//! signal, output that cannot be read back) returns `Error`. None of
//! them returns `Consistent`. That asymmetry is the whole point: the
//! caller turns `Error` into `Indeterminate`, and `Consistent` into a
//! real claim about the ontology, so a driver that quietly answered
//! `Consistent` when it had learned nothing would manufacture
//! agreement out of a broken CI setup.
//!
//! The stdout and stderr of each step go to FILES in `workdir`, not to
//! pipes. With a pipe, a child that outstrips the pipe buffer blocks
//! forever while this side is in `try_wait`, which is a hang the
//! deadline could not even interrupt cleanly; with files there is no
//! buffer to fill.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The substring ROBOT prints when the reasoner finds a clash. The
/// real line is `ERROR org.obolibrary.robot.ReasonerHelper - The
/// ontology is inconsistent. TIP: use a tool like Protege to find
/// explanations`, on stdout. Matched as a substring so the timestamp,
/// the logger name and the TIP cannot break it.
pub const INCONSISTENT_MARKER: &str = "ontology is inconsistent";

/// How long one question may take before it is abandoned as an
/// `Error`.
///
/// Measured on this repository's own hardware: merge plus reason over
/// real SULO (17 classes, 18 object properties) takes about 0.9
/// seconds wall clock for BOTH steps together, including two JVM
/// starts. 120 seconds is therefore two orders of magnitude of
/// headroom for a question that is merely slow, and still a bound
/// short enough that a pathological one cannot sit in CI until the
/// job's own timeout kills it with no report at all.
///
/// An expired deadline is an `Error`, never a `Consistent`. See the
/// module doc, point 3.
pub const HERMIT_DEADLINE: Duration = Duration::from_secs(120);

/// How often the driver looks to see whether ROBOT has finished.
const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// The command used to run the jar.
const JAVA: &str = "java";

/// What HermiT said, or why it did not say anything.
///
/// `Error` is a first-class answer, not an internal detail: ruling 3
/// of the plan is that a question HermiT could not answer is
/// `Indeterminate`, never agreement, and that ruling can only be
/// enforced if "could not answer" survives all the way out of this
/// function instead of collapsing into one of the two verdicts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HermitAnswer {
    /// HermiT found a model.
    Consistent,
    /// HermiT found a clash. This is the answer soundness cannot
    /// manufacture and incompleteness cannot hide.
    Inconsistent,
    /// Nothing was learned. Carries the reason, verbatim where ROBOT
    /// produced one.
    Error(String),
}

/// Ask HermiT whether `ontology` merged with `extra` is consistent.
///
/// `workdir` is created if it does not exist and is used for the
/// merged ontology, the reasoned output, and the captured stdout and
/// stderr of both steps. Give each concurrent question its own
/// directory: this function makes no attempt to keep two callers
/// sharing one workdir from overwriting each other's files.
///
/// Uses [`HERMIT_DEADLINE`]. See [`consistency_with_deadline`] when a
/// caller needs its own budget.
pub fn consistency(
    robot: &Path,
    ontology: &Path,
    extra: &[PathBuf],
    workdir: &Path,
) -> HermitAnswer {
    consistency_with_deadline(robot, ontology, extra, workdir, HERMIT_DEADLINE)
}

/// [`consistency`] with an explicit time budget covering BOTH steps.
///
/// Split out from `consistency` so the deadline path is reachable from
/// a test in under a second. A deadline guard that has never been
/// watched to fire is not a guard, and this one has a specific way of
/// failing wrong: were the expiry to return `Consistent`, every
/// question in a CI job whose JVM was thrashing would come back
/// "consistent", agree with rustdl's "not entailed", and go green.
pub fn consistency_with_deadline(
    robot: &Path,
    ontology: &Path,
    extra: &[PathBuf],
    workdir: &Path,
    deadline: Duration,
) -> HermitAnswer {
    let started = Instant::now();

    if let Err(e) = std::fs::create_dir_all(workdir) {
        return HermitAnswer::Error(format!(
            "cannot create the HermiT working directory {}: {e}",
            workdir.display()
        ));
    }

    let (merge, reason) = two_step_plan(ontology, extra, workdir);

    // Step 1 of 2. See the module doc: this MUST be its own process.
    match run(robot, &merge, workdir, "merge", deadline, started) {
        Err(e) => return HermitAnswer::Error(e),
        Ok(step) if !step.succeeded() => {
            return HermitAnswer::Error(format!(
                "robot merge failed ({}) before HermiT was reached, so nothing was \
                 learned about this ontology: {}",
                describe_status(step.code),
                trimmed(&step.output)
            ));
        }
        Ok(_) => {}
    }

    // Step 2 of 2, over the file step 1 wrote.
    let step = match run(robot, &reason, workdir, "reason", deadline, started) {
        Ok(s) => s,
        Err(e) => return HermitAnswer::Error(e),
    };

    classify_reason(step.code, &step.output)
}

/// Read the `reason` step's outcome. The whole verdict lives here, so
/// it is a pure function over the two things a caller can observe:
/// the exit status and the combined output.
///
/// Four arms, and only one of them says `Consistent`:
///
/// * exit 0 with no marker: consistent.
/// * non-zero with the marker: inconsistent (the measured shape).
/// * non-zero without the marker: an invocation error. NOT a verdict.
///   This is the arm `--output /dev/null` lands in, and treating it as
///   "inconsistent" (the tempting `status.code() == Some(1)` reading)
///   would turn every broken invocation into a fake detected clash.
/// * exit 0 WITH the marker: contradictory, so no answer is claimed.
///   Unreachable against ROBOT 1.9.7 and kept deliberately: the two
///   signals disagreeing means the classifier's premise is wrong, and
///   the only honest output then is `Error`. Erring towards `Error`
///   costs an `Indeterminate`; erring towards a verdict would report a
///   reasoner result nothing actually established.
#[must_use]
pub fn classify_reason(code: Option<i32>, output: &str) -> HermitAnswer {
    let reported = output.contains(INCONSISTENT_MARKER);
    match (code, reported) {
        (Some(0), false) => HermitAnswer::Consistent,
        (Some(0), true) => HermitAnswer::Error(format!(
            "robot reason exited 0 but also printed {INCONSISTENT_MARKER:?}; the two \
             signals contradict each other, so no answer is claimed: {}",
            trimmed(output)
        )),
        (Some(_), true) => HermitAnswer::Inconsistent,
        (Some(n), false) => HermitAnswer::Error(format!(
            "robot reason exited {n} without reporting an inconsistency, so this is an \
             invocation error and not a verdict: {}",
            trimmed(output)
        )),
        (None, _) => HermitAnswer::Error(format!(
            "robot reason was killed by a signal, so its exit status says nothing: {}",
            trimmed(output)
        )),
    }
}

/// The result of running one step.
struct Finished {
    /// `None` when the process was killed by a signal.
    code: Option<i32>,
    /// stdout and stderr, concatenated.
    output: String,
}

impl Finished {
    fn succeeded(&self) -> bool {
        self.code == Some(0)
    }
}

/// Build the two argv vectors, in order, sharing the merged file.
///
/// The invariant a test pins: `reason`'s `--input` is `merge`'s
/// `--output`, and neither argv mentions the other's command. That is
/// what makes this the two-step form rather than the chained one; see
/// the module doc for what the chained form measures as.
fn two_step_plan(
    ontology: &Path,
    extra: &[PathBuf],
    workdir: &Path,
) -> (Vec<OsString>, Vec<OsString>) {
    let merged = workdir.join("merged.ttl");
    let reasoned = workdir.join("reasoned.ttl");

    let mut merge: Vec<OsString> = vec!["merge".into()];
    for input in std::iter::once(ontology).chain(extra.iter().map(PathBuf::as_path)) {
        merge.push("--input".into());
        merge.push(input.as_os_str().to_os_string());
    }
    merge.push("--output".into());
    merge.push(merged.clone().into_os_string());

    let reason: Vec<OsString> = vec![
        "reason".into(),
        "--reasoner".into(),
        "hermit".into(),
        "--input".into(),
        merged.into_os_string(),
        "--output".into(),
        reasoned.into_os_string(),
    ];

    (merge, reason)
}

/// Run one step, bounded by what is left of `deadline` since
/// `started`.
///
/// `Err` means the step did not produce a usable status: it never
/// started, it outlived the deadline, or its output could not be read
/// back. Every one of those is a reason to say nothing, so they all
/// surface as `HermitAnswer::Error` at the call site.
fn run(
    robot: &Path,
    argv: &[OsString],
    workdir: &Path,
    name: &str,
    deadline: Duration,
    started: Instant,
) -> Result<Finished, String> {
    let out_path = workdir.join(format!("{name}.out"));
    let err_path = workdir.join(format!("{name}.err"));

    let expired = || started.elapsed() >= deadline;
    if expired() {
        return Err(format!(
            "the {deadline:?} deadline for this question expired before robot {name} \
             could be started, so nothing was learned"
        ));
    }

    let stdout = std::fs::File::create(&out_path)
        .map_err(|e| format!("cannot create {}: {e}", out_path.display()))?;
    let stderr = std::fs::File::create(&err_path)
        .map_err(|e| format!("cannot create {}: {e}", err_path.display()))?;

    let mut child = Command::new(JAVA)
        .arg("-jar")
        .arg(robot)
        .args(argv)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn()
        .map_err(|e| {
            format!(
                "cannot run `{JAVA} -jar {}`: {e}. The differential needs a JVM and a \
                 ROBOT jar; neither is on the default path of this harness",
                robot.display()
            )
        })?;

    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(e) => return Err(format!("cannot wait for robot {name}: {e}")),
        }
        if expired() {
            // Killed rather than left running: a JVM abandoned in CI
            // holds the file handles and the CPU for the rest of the
            // job.
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "robot {name} exceeded the {deadline:?} deadline for this question and \
                 was killed, so nothing was learned. An expired deadline is an error, \
                 not a consistency verdict"
            ));
        }
        std::thread::sleep(POLL_INTERVAL);
    };

    let mut output = read_log(&out_path)?;
    output.push_str(&read_log(&err_path)?);

    Ok(Finished {
        code: status.code(),
        output,
    })
}

/// Read one captured stream. Lossy on purpose: a stack trace with a
/// stray byte in it must still reach the reader, and this text is only
/// ever grepped or printed.
fn read_log(path: &Path) -> Result<String, String> {
    std::fs::read(path)
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .map_err(|e| {
            format!(
                "robot ran but its output at {} could not be read back ({e}), so its \
                 answer cannot be classified",
                path.display()
            )
        })
}

fn describe_status(code: Option<i32>) -> String {
    match code {
        Some(n) => format!("exit {n}"),
        None => "killed by a signal".to_string(),
    }
}

/// Keep an error message readable without hiding the part that names
/// the problem. ROBOT's own errors are short; a stack trace under
/// `-vvv` is not.
fn trimmed(output: &str) -> String {
    const LIMIT: usize = 2000;
    let text = output.trim();
    if text.is_empty() {
        return "(no output)".to_string();
    }
    match text.char_indices().nth(LIMIT) {
        None => text.to_string(),
        Some((cut, _)) => format!("{}... ({} bytes total)", &text[..cut], text.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Render an argv for an assertion message. Lossy, like
    /// `read_log`.
    fn show(argv: &[OsString]) -> Vec<String> {
        argv.iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    // -----------------------------------------------------------
    // Classification. No JVM involved: these are the four arms of
    // the measured result that the exit code alone cannot express.
    // -----------------------------------------------------------

    #[test]
    fn exit_zero_without_the_marker_is_consistent() {
        assert_eq!(classify_reason(Some(0), ""), HermitAnswer::Consistent);
    }

    #[test]
    fn the_marker_with_exit_one_is_inconsistent() {
        let real = "2026-08-26 15:41:47,931 ERROR org.obolibrary.robot.ReasonerHelper - \
                    The ontology is inconsistent. TIP: use a tool like Protege to find \
                    explanations\n";
        assert_eq!(classify_reason(Some(1), real), HermitAnswer::Inconsistent);
    }

    /// The arm that keeps the differential honest. This is the
    /// `--output /dev/null` output, measured: exit 1, no marker.
    /// Reading exit 1 as "inconsistent" would turn every broken
    /// invocation into a fabricated clash, and under the
    /// non-entailment encoding a fabricated clash reads as "entailed".
    #[test]
    fn a_non_zero_exit_without_the_marker_is_an_error_not_a_verdict() {
        let real = "INVALID FORMAT ERROR unknown format: /dev/null\n\
                    For details see: http://robot.obolibrary.org/errors#invalid-format-error\n";
        match classify_reason(Some(1), real) {
            HermitAnswer::Error(msg) => {
                assert!(
                    msg.contains("invocation error") && msg.contains("unknown format"),
                    "the error must say it is not a verdict and quote ROBOT: {msg}"
                );
            }
            other => panic!("exit 1 with no inconsistency message must be Error, got {other:?}"),
        }
    }

    #[test]
    fn a_signal_death_is_an_error() {
        match classify_reason(None, "") {
            HermitAnswer::Error(msg) => assert!(msg.contains("killed by a signal"), "{msg}"),
            other => panic!("a signalled process must be Error, got {other:?}"),
        }
    }

    /// Contradictory signals claim nothing. Unreachable against ROBOT
    /// 1.9.7; see `classify_reason` for why it is kept.
    #[test]
    fn exit_zero_with_the_marker_claims_nothing() {
        match classify_reason(Some(0), "The ontology is inconsistent.") {
            HermitAnswer::Error(msg) => assert!(msg.contains("contradict"), "{msg}"),
            other => panic!("contradictory signals must be Error, got {other:?}"),
        }
    }

    // -----------------------------------------------------------
    // The two-step form (measured result 1).
    // -----------------------------------------------------------

    /// Pins the shape a future maintainer is most likely to
    /// "simplify" back into the chained form, which measures as
    /// permanently CONSISTENT on the one case rustdl cannot decide.
    #[test]
    fn the_plan_is_two_steps_sharing_a_merged_file() {
        let (merge, reason) = two_step_plan(
            Path::new("/o/sulo.ttl"),
            &[PathBuf::from("/o/data.ttl")],
            Path::new("/w"),
        );
        let merge = show(&merge);
        let reason = show(&reason);

        assert_eq!(
            merge,
            vec![
                "merge",
                "--input",
                "/o/sulo.ttl",
                "--input",
                "/o/data.ttl",
                "--output",
                "/w/merged.ttl"
            ],
            "step 1 must be a plain merge to a file"
        );
        assert_eq!(
            reason,
            vec![
                "reason",
                "--reasoner",
                "hermit",
                "--input",
                "/w/merged.ttl",
                "--output",
                "/w/reasoned.ttl"
            ],
            "step 2 must reason over the file step 1 wrote"
        );

        // The invariant, stated as such rather than left implicit in
        // the two literals above.
        assert!(
            !merge.iter().any(|a| a == "reason"),
            "the merge step must not chain into reason: chaining measures as CONSISTENT \
             on the data-range case, which is the wrong answer"
        );
        assert!(
            !reason.iter().any(|a| a == "merge"),
            "the reason step must not re-merge"
        );
        let merge_output = merge.last().expect("merge argv is non-empty");
        assert!(
            reason.contains(merge_output),
            "step 2 must read exactly the file step 1 wrote ({merge_output})"
        );
    }

    #[test]
    fn every_extra_input_reaches_the_merge() {
        let (merge, _) = two_step_plan(
            Path::new("/o/sulo.ttl"),
            &[PathBuf::from("/o/a.ttl"), PathBuf::from("/o/b.ttl")],
            Path::new("/w"),
        );
        let merge = show(&merge);
        for wanted in ["/o/sulo.ttl", "/o/a.ttl", "/o/b.ttl"] {
            assert!(
                merge.iter().any(|a| a == wanted),
                "{wanted} must be merged, or the question would be asked of the wrong \
                 ontology: {merge:?}"
            );
        }
    }

    // -----------------------------------------------------------
    // Failure routes that must not become Consistent.
    // -----------------------------------------------------------

    /// No JVM is needed: the deadline is already spent, so the merge
    /// step never starts.
    #[test]
    fn an_expired_deadline_is_an_error_before_anything_is_spawned() {
        let workdir = std::env::temp_dir().join(format!(
            "sulo-testharness-hermit-deadline-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&workdir);
        let answer = consistency_with_deadline(
            Path::new("/nonexistent/robot.jar"),
            Path::new("/nonexistent/sulo.ttl"),
            &[],
            &workdir,
            Duration::ZERO,
        );
        match answer {
            HermitAnswer::Error(msg) => assert!(msg.contains("deadline"), "{msg}"),
            other => panic!("a spent deadline must be Error, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&workdir);
    }

    /// A jar path that cannot be run is an error, not a green
    /// "consistent". Needs no jar and no reachable java: if `java` is
    /// absent the spawn fails, and if it is present the JVM refuses
    /// the missing jar, and both are `Error`.
    #[test]
    fn an_unrunnable_jar_is_an_error() {
        let workdir = std::env::temp_dir().join(format!(
            "sulo-testharness-hermit-nojar-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&workdir);
        let answer = consistency(
            Path::new("/nonexistent/robot.jar"),
            Path::new("tests/fixtures/clean.ttl"),
            &[],
            &workdir,
        );
        assert!(
            matches!(answer, HermitAnswer::Error(_)),
            "an unrunnable jar must be Error, got {answer:?}"
        );
        let _ = std::fs::remove_dir_all(&workdir);
    }
}
