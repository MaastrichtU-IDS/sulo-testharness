//! The pinned set of KNOWN divergences, diffed in BOTH directions.
//!
//! `suites/sulo.divergences` is to the differential what
//! `suites/sulo.golden` is to the closure: a checked-in, sorted,
//! diffable record of a state somebody reviewed and signed off on.
//!
//! # Why a pin exists at all
//!
//! The differential exits 5 on the real suite, and correctly: rustdl
//! and HermiT genuinely disagree about `timeinstant-datarange`,
//! because horned-owl drops the data-range axiom on load. That is a
//! true, documented, expected disagreement, and it is the whole reason
//! the case is deferred.
//!
//! But a CI job that is permanently red gets muted, and a muted alarm
//! is this project's recurring defect shape (a check that cannot fail)
//! wearing different clothes. It is not enough that the job CAN fail;
//! it has to be capable of going green when the world is as
//! documented, so that red means "something changed" rather than
//! "Monday".
//!
//! # The three outcomes, and why the second direction is the point
//!
//! * A divergence the pin describes, observed again: matched. Exit 0.
//! * A divergence NOT in the pin: exit 5. Two reasoners disagree about
//!   something nobody has reviewed.
//! * A divergence IN the pin that no longer occurs: exit 4, NOT a
//!   quiet pass. It means rustdl gained a capability, or SULO changed,
//!   or the case moved. Either way the pin is now a lie, and the news
//!   that the gap closed is exactly the news this job exists to
//!   deliver. A pin that only caught new divergences would silently
//!   absorb that day.
//!
//! `tests/deferred.rs` and the six group `EXPECTED` tables already
//! apply this both-ways discipline to their own tables; this is the
//! same discipline over a file.
//!
//! # Why 5 and 4 are different codes here
//!
//! Exit 5 means "the two reasoners disagree" and exit 4 means "a
//! baseline no longer describes reality, re-baseline deliberately".
//! An UNPINNED divergence is squarely the first: an unreviewed
//! disagreement exists right now. A STALE pin is squarely the second,
//! and is in fact the opposite news: the reasoners now AGREE about
//! something they used to disagree about. Reporting both as 5 would
//! give opposite findings the same code and send the reader looking
//! for a disagreement that is not there. So a stale pin exits 4,
//! exactly as golden drift does, and for the same reason: the fix is
//! a reviewed edit to a checked-in baseline, not an investigation into
//! a reasoner.
//!
//! Precedence, when a run holds more than one: 5 over 4 over 3 over 0.
//! An unreviewed disagreement outranks a stale baseline because it is
//! the live finding; both outrank an unanswered question.
//!
//! # An Indeterminate is never a match, and never a disappearance
//!
//! A question HermiT could not answer is not evidence that a
//! documented disagreement still holds. It is equally not evidence
//! that it stopped holding. So a pinned divergence whose question came
//! back [`Comparison::Indeterminate`] is reported as UNCONFIRMED and
//! exits 3, not as stale (exit 4, which would send the reader hunting
//! for a capability rustdl did not gain) and certainly not as matched.
//! A broken jar therefore turns every pin unconfirmed and the run
//! exits 3, which is loud and true, rather than green.
//!
//! # What the pin does NOT record
//!
//! It records rustdl's version, because a rustdl bump is the single
//! most likely legitimate cause of a divergence closing, and a
//! mismatch there is reported as re-baseline required rather than
//! blamed on SULO.
//!
//! It does NOT record ROBOT's or HermiT's version. The differential
//! never asks ROBOT for one, and adding a version probe would mean an
//! extra JVM start on every run. The consequence, stated rather than
//! hidden: a HermiT change that closes the gap surfaces here as a
//! stale pin (exit 4) with no version line to explain it. That is
//! still a deliberate, reviewed re-baseline, just one whose cause the
//! reader has to work out. The workflow pins `ROBOT_VERSION`, which is
//! where that constant actually lives today.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::differential::{Answer, Asked, Comparison, Origin};
use crate::golden::REASONER_VERSION;

/// The flag that re-baselines the pin. Named in messages so the reader
/// is told the one deliberate way out.
pub const ACCEPT_FLAG: &str = "--accept-divergences";

/// One documented disagreement.
///
/// # What makes two divergences "the same"
///
/// All five fields. Case id plus check name locates the question;
/// origin says which of the three check kinds produced it (and
/// therefore what a divergence MEANS, since
/// `differential::explain_divergence` reads it); and BOTH answers say
/// what the disagreement actually is.
///
/// Matching on less would build a pin that cannot fail in the way that
/// matters most. Matching on the case id alone, or on case plus check,
/// would let a divergence whose answers FLIPPED still count as
/// matched: the day rustdl started claiming a proof HermiT refutes
/// (`inconsistent` vs `consistent`, the unsoundness direction) on the
/// same check that today shows the harmless incompleteness direction,
/// a coarser pin would report it as the documented, expected,
/// signed-off disagreement and exit 0. That is the alarming direction
/// of the alarming finding, absorbed silently.
///
/// The one field deliberately NOT part of the identity is
/// `Provenance::asked`, the prose description of the question. It is a
/// sentence written for a human, it is free to be reworded, and
/// rewording it changes nothing about which question was put or what
/// the two reasoners said. Including it would make every report
/// tidy-up a spurious re-baseline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinnedDivergence {
    pub case_id: String,
    pub check: String,
    pub origin: Origin,
    pub rustdl: Answer,
    pub hermit: Answer,
}

impl PinnedDivergence {
    /// This divergence as one tab-separated line of the pin file.
    #[must_use]
    pub fn line(&self) -> String {
        format!(
            "{}\t{}\t{}\t{}\t{}",
            self.case_id,
            self.check,
            self.origin.token(),
            self.rustdl.token(),
            self.hermit.token()
        )
    }

    /// Parse one line. `Err` names what was wrong, and callers turn
    /// that into re-baseline required: a pin file this cannot read is
    /// never trusted enough to compare against, and must never be
    /// silently treated as an empty pin (which would make every real
    /// divergence "unpinned" for the wrong reason, or, on the day
    /// there are none, make an unreadable file pass).
    pub fn parse(line: &str) -> Result<Self, String> {
        let fields: Vec<&str> = line.split('\t').collect();
        let [case_id, check, origin, rustdl, hermit] = fields.as_slice() else {
            return Err(format!(
                "expected 5 tab-separated fields (case, check, origin, rustdl, hermit), \
                 found {}: {line:?}",
                fields.len()
            ));
        };
        Ok(PinnedDivergence {
            case_id: (*case_id).to_string(),
            check: (*check).to_string(),
            origin: Origin::from_token(origin)
                .ok_or_else(|| format!("unknown origin {origin:?} in {line:?}"))?,
            rustdl: Answer::from_token(rustdl)
                .ok_or_else(|| format!("unknown rustdl answer {rustdl:?} in {line:?}"))?,
            hermit: Answer::from_token(hermit)
                .ok_or_else(|| format!("unknown HermiT answer {hermit:?} in {line:?}"))?,
        })
    }
}

// Ordered by the rendered line, so the file sorts the way a reader
// reads it (by case, then check) and a `git diff` of a re-baseline is
// legible. Deriving `Ord` instead would sort by `Origin`'s and
// `Answer`'s declaration order, which means nothing to anybody.
impl Ord for PinnedDivergence {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.line().cmp(&other.line())
    }
}

impl PartialOrd for PinnedDivergence {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// The both-directions diff of one run against the pin.
///
/// Four buckets rather than a boolean, because the four mean four
/// different things to the reader and map onto three different exit
/// codes. See [`pinned_exit_code`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PinDiff {
    /// Pinned, and observed again. The expected state.
    pub matched: Vec<PinnedDivergence>,
    /// Observed, and not pinned. Exit 5: something changed.
    pub unpinned: Vec<PinnedDivergence>,
    /// Pinned, not observed, and the question WAS answered. Exit 4:
    /// the pin is a lie and must be updated deliberately.
    pub stale: Vec<PinnedDivergence>,
    /// Pinned, not observed, and the question was never answered.
    /// Exit 3, carrying the reason nothing was learned.
    pub unconfirmed: Vec<(PinnedDivergence, String)>,
}

/// Outcome of comparing a run against its pin file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PinOutcome {
    /// The pin was read and diffed. Read [`PinDiff`] for what came of
    /// it; `Compared` on its own is NOT a pass.
    Compared(PinDiff),
    /// The pin was rewritten from this run, because [`ACCEPT_FLAG`]
    /// was passed.
    Rebaselined(PathBuf),
    /// The pin cannot be compared against, but the run itself was
    /// fine: exit 4, review and re-accept.
    RebaselineRequired(String),
    /// A harness, IO or configuration failure. Exit 2, and never a
    /// verdict about either reasoner. Kept distinct from
    /// `RebaselineRequired` for the reason `golden::GoldenOutcome`
    /// keeps them distinct: reporting a missing file as drift trains
    /// an operator to reach for the accept flag, which is exactly the
    /// wrong reflex for a wrong path.
    Error(String),
}

/// The divergences one run observed, as pin rows.
///
/// [`Comparison::Indeterminate`] and [`Comparison::Agree`] contribute
/// nothing, which is what makes an unanswered question incapable of
/// standing in for a documented disagreement.
#[must_use]
pub fn observed(asked: &[Asked]) -> BTreeSet<PinnedDivergence> {
    asked
        .iter()
        .filter_map(|a| match &a.comparison {
            Comparison::Divergence {
                question,
                rustdl,
                hermit,
            } => Some(PinnedDivergence {
                case_id: question.case_id.clone(),
                check: question.check.clone(),
                origin: question.origin,
                rustdl: *rustdl,
                hermit: *hermit,
            }),
            Comparison::Agree { .. } | Comparison::Indeterminate { .. } => None,
        })
        .collect()
}

/// Serialise a pin file: three header lines, a column legend, then the
/// sorted rows.
#[must_use]
pub fn document(suite: &Path, divergences: &BTreeSet<PinnedDivergence>) -> String {
    let mut out = format!(
        "# suite: {}\n\
         # reasoner: {REASONER_VERSION}\n\
         # generated by sulo-testharness; regenerate with {ACCEPT_FLAG}\n\
         # case\tcheck\torigin\trustdl\thermit\n",
        suite.display()
    );
    for d in divergences {
        out.push_str(&d.line());
        out.push('\n');
    }
    out
}

/// Parse a pin file's rows. Comment and blank lines are skipped; every
/// other line must parse.
fn parse_rows(text: &str) -> Result<BTreeSet<PinnedDivergence>, String> {
    text.lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(PinnedDivergence::parse)
        .collect()
}

/// The reasoner version a pin file was produced with.
fn parse_reasoner(text: &str) -> Option<&str> {
    text.lines().find_map(|l| l.strip_prefix("# reasoner: "))
}

/// The suite a pin file describes, exactly as `--suite` spelled it.
fn parse_suite(text: &str) -> Option<&str> {
    text.lines().find_map(|l| l.strip_prefix("# suite: "))
}

/// Best-effort absolute form, for messages only. Same helper, same
/// reason, as `golden::display_path`.
fn display_path(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Compare this run's divergences against the pin, or re-baseline it.
///
/// With `accept` true this ALWAYS rewrites the file and returns
/// [`PinOutcome::Rebaselined`], with one exception below: this is the
/// single deliberate, explicit way to accept a new set of divergences,
/// exactly as `--accept-golden` is for the closure.
///
/// The exception, and it is not decoration: a run holding ANY
/// [`Comparison::Indeterminate`] is refused. Such a run did not learn
/// whether the pinned divergences still occur, so re-baselining from
/// it would write a pin asserting less than the truth, and the most
/// likely way to get one is a broken jar, which would silently ERASE
/// every documented divergence and leave a permanently green job.
///
/// With `accept` false and no file present this returns
/// [`PinOutcome::Error`], never a silent write: falling back to
/// "write it and pass" on a merely-absent path would let a wrong
/// `--divergences` argument disable the pin entirely while still
/// exiting 0.
pub fn check_pin(asked: &[Asked], suite: &Path, path: &Path, accept: bool) -> PinOutcome {
    let seen = observed(asked);

    if accept {
        let unanswered = asked
            .iter()
            .filter(|a| matches!(a.comparison, Comparison::Indeterminate { .. }))
            .count();
        if unanswered > 0 {
            return PinOutcome::Error(format!(
                "refusing to re-baseline {} from a run with {unanswered} unanswered \
                 question(s). A question HermiT could not answer is not evidence that a \
                 documented divergence stopped occurring, so this run does not know what \
                 belongs in the pin; writing it would quietly drop divergences the run \
                 never tested for. Fix the Indeterminates first (a broken jar is the \
                 usual cause) and re-run with {ACCEPT_FLAG}.",
                display_path(path).display()
            ));
        }
        return match std::fs::write(path, document(suite, &seen)) {
            Ok(()) => PinOutcome::Rebaselined(path.to_path_buf()),
            Err(e) => PinOutcome::Error(format!("could not write the pin file: {e}")),
        };
    }

    if !path.exists() {
        return PinOutcome::Error(format!(
            "no pin file at {}; run with {ACCEPT_FLAG} to create one deliberately. A \
             missing pin is never treated as an empty one: that would make an unpinned \
             divergence look reviewed on the day there happens to be none.",
            display_path(path).display()
        ));
    }

    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => return PinOutcome::Error(format!("could not read the pin file: {e}")),
    };

    // The corpus the pin describes. `--filter` is already refused
    // (see `run_differential`) because it silently narrows within a
    // corpus; this catches the other half, a run pointed at a
    // DIFFERENT corpus, where every pin entry outside it would
    // otherwise read as a divergence that stopped occurring. A
    // mismatch is never silent: worst case somebody spelled the same
    // directory two ways and gets told to re-accept.
    match parse_suite(&text) {
        None => {
            return PinOutcome::RebaselineRequired(
                "the pin file has no '# suite: ...' line, so there is no telling which \
                 corpus it describes. A pin whose provenance cannot be read is not \
                 trusted enough to compare against."
                    .to_string(),
            );
        }
        Some(s) if s != suite.display().to_string() => {
            return PinOutcome::RebaselineRequired(format!(
                "the pin describes the suite {s:?}, this run was over {:?}. A pin is a \
                 claim about a WHOLE corpus, so comparing it against a different one \
                 would report every entry outside that corpus as a divergence that \
                 stopped occurring. If the two really are the same directory spelled two \
                 ways, spell it the pin's way or re-run with {ACCEPT_FLAG}.",
                suite.display().to_string()
            ));
        }
        Some(_) => {}
    }

    match parse_reasoner(&text) {
        None => {
            return PinOutcome::RebaselineRequired(
                "the pin file's header is missing or malformed (expected a \
                 '# reasoner: ...' line). A pin whose provenance cannot be read is not \
                 trusted enough to compare against."
                    .to_string(),
            );
        }
        Some(v) if v != REASONER_VERSION => {
            return PinOutcome::RebaselineRequired(format!(
                "the pin was recorded against {v}, this run used {REASONER_VERSION}. A \
                 reasoner change is the most likely legitimate cause of a divergence \
                 opening or closing, so the pin is reviewed rather than blamed on SULO; \
                 re-run with {ACCEPT_FLAG}."
            ));
        }
        Some(_) => {}
    }

    let pinned = match parse_rows(&text) {
        Ok(p) => p,
        Err(e) => {
            return PinOutcome::RebaselineRequired(format!(
                "the pin file at {} could not be parsed: {e}",
                display_path(path).display()
            ));
        }
    };

    PinOutcome::Compared(diff(asked, &seen, &pinned))
}

/// The both-directions diff proper.
///
/// Split out from [`check_pin`] so every direction can be exercised
/// over synthetic runs with no JVM anywhere near the test.
#[must_use]
pub fn diff(
    asked: &[Asked],
    observed: &BTreeSet<PinnedDivergence>,
    pinned: &BTreeSet<PinnedDivergence>,
) -> PinDiff {
    let mut out = PinDiff {
        matched: observed.intersection(pinned).cloned().collect(),
        unpinned: observed.difference(pinned).cloned().collect(),
        ..PinDiff::default()
    };

    for missing in pinned.difference(observed) {
        // The pinned divergence was not seen. Before calling that a
        // disappearance, check whether the question was actually PUT
        // and ANSWERED. If HermiT could not answer it, this run
        // learned nothing about the pin either way, and saying "the
        // gap closed" would be the overstatement this project exists
        // to avoid.
        let unanswered = asked.iter().find_map(|a| match &a.comparison {
            Comparison::Indeterminate { question, reason }
                if question.case_id == missing.case_id && question.check == missing.check =>
            {
                Some(reason.clone())
            }
            _ => None,
        });
        match unanswered {
            Some(reason) => out.unconfirmed.push((missing.clone(), reason)),
            None => out.stale.push(missing.clone()),
        }
    }

    out
}

/// The process exit code for a run compared against a pin.
///
/// 5 an unpinned divergence, 4 a stale pin, 3 an unconfirmed pin or
/// any other unanswered question, 0 only when every divergence was
/// pinned, every pin was observed, and every question was answered.
///
/// The run's own Indeterminates are counted here as well as the pin's,
/// because `differential_exit_code` is NOT consulted on this path: a
/// MATCHED divergence must not raise a 5, and reusing that function
/// would make it do so.
#[must_use]
pub fn pinned_exit_code(asked: &[Asked], diff: &PinDiff) -> i32 {
    if !diff.unpinned.is_empty() {
        5
    } else if !diff.stale.is_empty() {
        4
    } else if !diff.unconfirmed.is_empty()
        || asked
            .iter()
            .any(|a| matches!(a.comparison, Comparison::Indeterminate { .. }))
    {
        3
    } else {
        0
    }
}

/// The pin section of the human-readable report.
#[must_use]
pub fn render(path: &Path, diff: &PinDiff) -> String {
    let mut out = format!("\npinned divergences: {}\n", path.display());

    for d in &diff.unpinned {
        out.push_str(&format!(
            "  UNPINNED  {} / {}\n    rustdl: {}, HermiT: {}\n    This disagreement is not \
             in the pin. Either it is new, or a pinned one changed its answers. Review it, \
             then record it with {ACCEPT_FLAG} if it is expected.\n",
            d.case_id, d.check, d.rustdl, d.hermit
        ));
    }

    for d in &diff.stale {
        out.push_str(&format!(
            "  STALE  {} / {}\n    pinned as rustdl: {}, HermiT: {}, and it no longer \
             occurs.\n    Either the two reasoners now agree here (rustdl gained a \
             capability, or SULO changed), or the question moved out from under the pin \
             (the case was renamed, the check was renamed, or this harness reclassified \
             its origin). The pin is a lie until somebody works out which and updates it \
             with {ACCEPT_FLAG}.\n",
            d.case_id, d.check, d.rustdl, d.hermit
        ));
    }

    for (d, reason) in &diff.unconfirmed {
        out.push_str(&format!(
            "  UNCONFIRMED  {} / {}\n    pinned as rustdl: {}, HermiT: {}, and this run \
             could not tell whether it still occurs: {reason}\n",
            d.case_id, d.check, d.rustdl, d.hermit
        ));
    }

    for d in &diff.matched {
        out.push_str(&format!(
            "  as pinned  {} / {}: rustdl {}, HermiT {}\n",
            d.case_id, d.check, d.rustdl, d.hermit
        ));
    }

    out.push_str(&format!(
        "  {} as pinned, {} unpinned, {} stale, {} unconfirmed\n",
        diff.matched.len(),
        diff.unpinned.len(),
        diff.stale.len(),
        diff.unconfirmed.len()
    ));
    out
}

/// The pin section as JSON, for the `--format json` payload.
#[must_use]
pub fn render_json(path: &Path, diff: &PinDiff) -> serde_json::Value {
    let row = |d: &PinnedDivergence| {
        serde_json::json!({
            "case": d.case_id,
            "check": d.check,
            "origin": d.origin.token(),
            "rustdl": d.rustdl.token(),
            "hermit": d.hermit.token(),
        })
    };
    serde_json::json!({
        "file": path.display().to_string(),
        "matched": diff.matched.iter().map(row).collect::<Vec<_>>(),
        "unpinned": diff.unpinned.iter().map(row).collect::<Vec<_>>(),
        "stale": diff.stale.iter().map(row).collect::<Vec<_>>(),
        "unconfirmed": diff.unconfirmed.iter().map(|(d, reason)| {
            let mut v = row(d);
            v.as_object_mut()
                .expect("json! built an object")
                .insert("reason".into(), reason.clone().into());
            v
        }).collect::<Vec<_>>(),
    })
}
