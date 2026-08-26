//! The pinned set of KNOWN divergences.
//!
//! Three layers, and the middle one is the reason this file exists:
//!
//! 1. **The file format.** Every token round-trips, and an unreadable
//!    pin is a loud refusal rather than an empty pin quietly compared
//!    against.
//! 2. **The diff, in BOTH directions, over synthetic runs.** A
//!    divergence that is not pinned fails; a pinned divergence that no
//!    longer occurs fails too. The second direction is the one most
//!    likely to be quietly wrong, because a pin that only catches new
//!    divergences passes every test somebody thinks to write for the
//!    first direction. It is tested here by pinning a divergence that
//!    does NOT occur and asserting the run fails.
//! 3. **The checked-in pin.** `suites/sulo.divergences` is diffed
//!    against a table in this file, so a casual `--accept-divergences`
//!    that absorbs a change cannot be committed without a test failing
//!    in the ordinary, jar-free CI job.
//!
//! No JVM anywhere in here. The end-to-end observation lives in
//! `tests/cli.rs`, which is jar-gated; this file is the layer that has
//! to keep working when nobody has Java installed.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use sulo_testharness::differential::{
    Answer, Asked, Comparison, DifferentialOptions, Origin, Provenance, render_json,
};
use sulo_testharness::divergences::{
    ACCEPT_FLAG, PinDiff, PinOutcome, PinnedDivergence, check_pin, diff, document, observed,
    pinned_exit_code,
};
use sulo_testharness::golden::REASONER_VERSION;
use sulo_testharness::manifest::load_case;
use sulo_testharness::suite::discover;

const SUITE: &str = "suites/sulo";
const PIN: &str = "suites/sulo.divergences";

// ---------------------------------------------------------------
// Fixtures.
// ---------------------------------------------------------------

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "sulo-testharness-pin-{}-{name}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("the scratch directory should be creatable");
    dir
}

fn provenance(case: &str, check: &str) -> Provenance {
    Provenance {
        case_id: case.to_string(),
        check: check.to_string(),
        asked: "prose a human reads and nothing compares".to_string(),
        origin: Origin::Gate,
    }
}

fn diverged(case: &str, check: &str, rustdl: Answer, hermit: Answer) -> Asked {
    Asked {
        provenance: provenance(case, check),
        comparison: Comparison::Divergence {
            question: provenance(case, check),
            rustdl,
            hermit,
        },
    }
}

fn agreed(case: &str, check: &str, answer: Answer) -> Asked {
    Asked {
        provenance: provenance(case, check),
        comparison: Comparison::Agree { answer },
    }
}

fn unanswered(case: &str, check: &str) -> Asked {
    Asked {
        provenance: provenance(case, check),
        comparison: Comparison::Indeterminate {
            question: provenance(case, check),
            reason: "HermiT gave no answer, so nothing was cross-checked: robot fell over"
                .to_string(),
        },
    }
}

fn pin(case: &str, check: &str, rustdl: Answer, hermit: Answer) -> PinnedDivergence {
    PinnedDivergence {
        case_id: case.to_string(),
        check: check.to_string(),
        origin: Origin::Gate,
        rustdl,
        hermit,
    }
}

fn set(entries: &[PinnedDivergence]) -> BTreeSet<PinnedDivergence> {
    entries.iter().cloned().collect()
}

/// The diff plus the exit code it produces, which is the pair every
/// test below actually cares about.
fn run(asked: &[Asked], pinned: &BTreeSet<PinnedDivergence>) -> (PinDiff, i32) {
    let d = diff(asked, &observed(asked), pinned);
    let code = pinned_exit_code(asked, &d);
    (d, code)
}

// ---------------------------------------------------------------
// 1: the file format.
// ---------------------------------------------------------------

/// Every `Answer` and every `Origin` survives a trip through the pin
/// file's tokens.
///
/// Enumerated rather than sampled: a variant added later without a
/// token would parse as `None`, and `None` on the WRITE side would be
/// a panic while `None` on the READ side is re-baseline required. Both
/// are loud, but only if some test walks every variant.
#[test]
fn every_answer_and_origin_token_round_trips() {
    for a in [
        Answer::Consistent,
        Answer::Inconsistent,
        Answer::Entailed,
        Answer::NotEntailed,
        Answer::Satisfiable,
        Answer::Unsatisfiable,
    ] {
        assert_eq!(
            Answer::from_token(a.token()),
            Some(a),
            "{a:?} does not round-trip through its token"
        );
    }
    for o in [Origin::Gate, Origin::Unrefuted, Origin::FailingPositive] {
        assert_eq!(
            Origin::from_token(o.token()),
            Some(o),
            "{o:?} does not round-trip through its token"
        );
    }
}

/// The tokens are a file format, so they are pinned literally. A
/// rewording of the report's prose (`Display`) must not silently
/// invalidate every checked-in pin, and this is what notices if the
/// two are ever collapsed into one string.
#[test]
fn the_tokens_are_pinned_and_are_not_the_display_prose() {
    assert_eq!(Answer::NotEntailed.token(), "not_entailed");
    assert_eq!(Answer::NotEntailed.to_string(), "not entailed");
    assert_eq!(Origin::FailingPositive.token(), "failing_positive");
    assert_eq!(
        Origin::FailingPositive.to_string(),
        "positive assertion rustdl could not prove"
    );
}

/// One row, written and read back, including a check name carrying the
/// spaces, colons, quotes and braces the real suite's check names carry.
#[test]
fn a_row_round_trips_through_the_line_format() {
    let d = PinnedDivergence {
        case_id: "non-subsumptions".to_string(),
        check: r#"Subsumption { sub: "https://w3id.org/sulo/Role", sup: "x" }"#.to_string(),
        origin: Origin::Unrefuted,
        rustdl: Answer::NotEntailed,
        hermit: Answer::Entailed,
    };
    assert_eq!(PinnedDivergence::parse(&d.line()), Ok(d));
}

#[test]
fn a_document_is_sorted_and_carries_its_provenance() {
    let text = document(
        Path::new(SUITE),
        &set(&[
            pin("zeta", "gate", Answer::Consistent, Answer::Inconsistent),
            pin("alpha", "gate", Answer::Consistent, Answer::Inconsistent),
        ]),
    );
    let rows: Vec<&str> = text.lines().filter(|l| !l.starts_with('#')).collect();
    assert_eq!(rows.len(), 2);
    assert!(rows[0].starts_with("alpha\t"), "{rows:?}");
    assert!(rows[1].starts_with("zeta\t"), "{rows:?}");
    assert!(
        text.contains(&format!("# reasoner: {REASONER_VERSION}\n")),
        "{text}"
    );
    assert!(text.contains(&format!("# suite: {SUITE}\n")), "{text}");
    assert!(text.contains(ACCEPT_FLAG), "{text}");
}

// ---------------------------------------------------------------
// 2: the diff, both directions.
// ---------------------------------------------------------------

/// The expected state: the pin describes the divergence, the run finds
/// it, and the run is GREEN. Without this the whole exercise is
/// pointless, because a permanently red job is a muted job.
#[test]
fn a_divergence_the_pin_describes_is_not_news() {
    let asked = [
        diverged("d", "gate", Answer::Consistent, Answer::Inconsistent),
        agreed("other", "gate", Answer::Consistent),
    ];
    let (d, code) = run(
        &asked,
        &set(&[pin("d", "gate", Answer::Consistent, Answer::Inconsistent)]),
    );
    assert_eq!(d.matched.len(), 1, "{d:?}");
    assert!(
        d.unpinned.is_empty() && d.stale.is_empty() && d.unconfirmed.is_empty(),
        "{d:?}"
    );
    assert_eq!(
        code, 0,
        "a run matching its pin exactly must be green: {d:?}"
    );
}

/// Direction one: something new.
#[test]
fn a_divergence_the_pin_does_not_describe_exits_five() {
    let asked = [diverged(
        "new",
        "gate",
        Answer::Consistent,
        Answer::Inconsistent,
    )];
    let (d, code) = run(&asked, &BTreeSet::new());
    assert_eq!(d.unpinned.len(), 1, "{d:?}");
    assert_eq!(
        code, 5,
        "an unreviewed disagreement between the two reasoners is exit 5: {d:?}"
    );
}

/// Direction two, and the whole point of ruling 12: a divergence the
/// pin describes that no longer occurs is a FAILURE, never a quiet
/// pass.
///
/// Proved by pinning a divergence that does not occur. The run agrees
/// on that very question, which is exactly the shape of "rustdl gained
/// a capability": the pin is now a lie and must be updated
/// deliberately.
#[test]
fn a_pinned_divergence_that_no_longer_occurs_exits_four() {
    let asked = [agreed("d", "gate", Answer::Inconsistent)];
    let (d, code) = run(
        &asked,
        &set(&[pin("d", "gate", Answer::Consistent, Answer::Inconsistent)]),
    );
    assert_eq!(d.stale.len(), 1, "{d:?}");
    assert!(
        d.matched.is_empty(),
        "an agreement is not a matched divergence: {d:?}"
    );
    assert_eq!(
        code, 4,
        "a documented divergence that stopped occurring must fail, not pass: {d:?}"
    );
}

/// The same direction, reached the other way: the case is gone from
/// the suite entirely, so the question was never even asked.
///
/// This is the shape of "the case moved or was renamed", and it must
/// not be a pass either: the pin would otherwise sit there describing
/// a case nothing runs.
#[test]
fn a_pin_naming_a_case_that_produced_no_question_is_stale() {
    let asked = [agreed("something-else", "gate", Answer::Consistent)];
    let (d, code) = run(
        &asked,
        &set(&[pin(
            "renamed-away",
            "gate",
            Answer::Consistent,
            Answer::Inconsistent,
        )]),
    );
    assert_eq!(d.stale.len(), 1, "{d:?}");
    assert_eq!(code, 4, "{d:?}");
}

/// Identity: a divergence whose ANSWERS flipped is not the same
/// divergence.
///
/// The pin describes the harmless direction (rustdl found no proof,
/// HermiT did: an incompleteness). The run observes the alarming one
/// (rustdl claims a proof HermiT refutes: an unsoundness). A pin
/// matching on case and check alone would call that expected and exit
/// 0, which would absorb the worst finding this harness can produce.
#[test]
fn a_divergence_whose_answers_flipped_is_not_the_pinned_one() {
    let asked = [diverged(
        "d",
        "gate",
        Answer::Inconsistent,
        Answer::Consistent,
    )];
    let (d, code) = run(
        &asked,
        &set(&[pin("d", "gate", Answer::Consistent, Answer::Inconsistent)]),
    );
    assert!(
        d.matched.is_empty(),
        "the answers flipped; this is a different disagreement: {d:?}"
    );
    assert_eq!(d.unpinned.len(), 1, "{d:?}");
    assert_eq!(d.stale.len(), 1, "{d:?}");
    assert_eq!(
        code, 5,
        "an unreviewed disagreement outranks the stale entry it displaced: {d:?}"
    );
}

/// Identity, the other coordinate: the same answers on a DIFFERENT
/// check of the same case are a different divergence.
#[test]
fn a_divergence_on_another_check_of_the_same_case_is_not_the_pinned_one() {
    let asked = [diverged(
        "d",
        "some other check",
        Answer::Consistent,
        Answer::Inconsistent,
    )];
    let (d, code) = run(
        &asked,
        &set(&[pin("d", "gate", Answer::Consistent, Answer::Inconsistent)]),
    );
    assert!(d.matched.is_empty(), "{d:?}");
    assert_eq!(code, 5, "{d:?}");
}

/// Ruling 3, carried into the pin: a question HermiT could not answer
/// is not evidence that a documented disagreement still holds.
///
/// It is equally not evidence that it stopped, so this is UNCONFIRMED
/// (exit 3), not stale (exit 4, which would send the reader hunting
/// for a rustdl capability that never arrived) and emphatically not
/// matched.
#[test]
fn an_unanswered_question_neither_confirms_nor_retires_a_pin() {
    let asked = [unanswered("d", "gate")];
    let (d, code) = run(
        &asked,
        &set(&[pin("d", "gate", Answer::Consistent, Answer::Inconsistent)]),
    );
    assert!(
        d.matched.is_empty(),
        "an Indeterminate must never count as a matched divergence: {d:?}"
    );
    assert!(
        d.stale.is_empty(),
        "an Indeterminate is not evidence the divergence went away: {d:?}"
    );
    assert_eq!(d.unconfirmed.len(), 1, "{d:?}");
    assert!(d.unconfirmed[0].1.contains("robot fell over"), "{d:?}");
    assert_eq!(code, 3, "{d:?}");
}

/// The same, from the other side: an Indeterminate never becomes an
/// OBSERVED divergence either, so it cannot fill a pin slot.
#[test]
fn an_indeterminate_is_never_an_observed_divergence() {
    assert!(observed(&[unanswered("d", "gate")]).is_empty());
    assert!(observed(&[agreed("d", "gate", Answer::Consistent)]).is_empty());
    assert_eq!(
        observed(&[diverged(
            "d",
            "gate",
            Answer::Consistent,
            Answer::Inconsistent
        )])
        .len(),
        1
    );
}

/// A run whose pin matches perfectly but which left some OTHER
/// question unanswered is exit 3, not 0. A green pin over a run that
/// could not answer half its questions would be the same overstatement
/// in a different place.
#[test]
fn a_matched_pin_does_not_cover_an_unanswered_question_elsewhere() {
    let asked = [
        diverged("d", "gate", Answer::Consistent, Answer::Inconsistent),
        unanswered("other", "gate"),
    ];
    let (d, code) = run(
        &asked,
        &set(&[pin("d", "gate", Answer::Consistent, Answer::Inconsistent)]),
    );
    assert_eq!(d.matched.len(), 1, "{d:?}");
    assert_eq!(
        code, 3,
        "an unanswered question is not everything agreed: {d:?}"
    );
}

/// Precedence, all four rows: 5 over 4 over 3 over 0.
#[test]
fn the_exit_code_ranks_an_unreviewed_disagreement_above_a_stale_pin() {
    let unpinned = PinDiff {
        unpinned: vec![pin("a", "gate", Answer::Consistent, Answer::Inconsistent)],
        stale: vec![pin("b", "gate", Answer::Consistent, Answer::Inconsistent)],
        unconfirmed: vec![(
            pin("c", "gate", Answer::Consistent, Answer::Inconsistent),
            "why".to_string(),
        )],
        ..PinDiff::default()
    };
    assert_eq!(pinned_exit_code(&[], &unpinned), 5);

    let stale = PinDiff {
        stale: unpinned.stale.clone(),
        unconfirmed: unpinned.unconfirmed.clone(),
        ..PinDiff::default()
    };
    assert_eq!(pinned_exit_code(&[], &stale), 4);

    let unconfirmed = PinDiff {
        unconfirmed: unpinned.unconfirmed.clone(),
        ..PinDiff::default()
    };
    assert_eq!(pinned_exit_code(&[], &unconfirmed), 3);

    assert_eq!(pinned_exit_code(&[], &PinDiff::default()), 0);
}

// ---------------------------------------------------------------
// 3: reading and writing the file, and the `--accept` posture.
// ---------------------------------------------------------------

/// A MISSING pin is never silently written, exactly as a missing
/// golden file is not. A wrong `--divergences` path must not disable
/// the pin while still exiting 0.
#[test]
fn a_missing_pin_file_is_an_error_never_a_silent_write() {
    let path = scratch("missing").join("nope.divergences");
    let _ = std::fs::remove_file(&path);
    let asked = [diverged(
        "d",
        "gate",
        Answer::Consistent,
        Answer::Inconsistent,
    )];
    match check_pin(&asked, Path::new(SUITE), &path, false) {
        PinOutcome::Error(m) => assert!(m.contains(ACCEPT_FLAG), "{m}"),
        other => panic!("a missing pin must be an Error, not {other:?}"),
    }
    assert!(
        !path.exists(),
        "nothing may be written without {ACCEPT_FLAG}"
    );
}

#[test]
fn accepting_writes_the_pin_and_the_next_run_matches_it() {
    let path = scratch("accept").join("written.divergences");
    let _ = std::fs::remove_file(&path);
    let asked = [
        diverged("d", "gate", Answer::Consistent, Answer::Inconsistent),
        agreed("other", "gate", Answer::Consistent),
    ];
    assert_eq!(
        check_pin(&asked, Path::new(SUITE), &path, true),
        PinOutcome::Rebaselined(path.clone())
    );
    match check_pin(&asked, Path::new(SUITE), &path, false) {
        PinOutcome::Compared(d) => {
            assert_eq!(d.matched.len(), 1, "{d:?}");
            assert_eq!(pinned_exit_code(&asked, &d), 0, "{d:?}");
        }
        other => panic!("{other:?}"),
    }
}

/// Re-baselining from a run that could not answer its questions is
/// refused.
///
/// The usual way to get an Indeterminate everywhere is a broken jar,
/// and accepting from such a run would write an EMPTY pin, erase every
/// documented divergence, and leave a permanently green job that had
/// asserted nothing. That is this project's recurring defect shape
/// arriving through the escape hatch built to avoid it.
#[test]
fn accepting_is_refused_when_the_run_left_a_question_unanswered() {
    let path = scratch("accept-unanswered").join("written.divergences");
    let _ = std::fs::remove_file(&path);
    let asked = [
        diverged("d", "gate", Answer::Consistent, Answer::Inconsistent),
        unanswered("other", "gate"),
    ];
    match check_pin(&asked, Path::new(SUITE), &path, true) {
        PinOutcome::Error(m) => assert!(
            m.contains("unanswered") && m.contains(ACCEPT_FLAG),
            "the refusal must say what is wrong and how to get past it: {m}"
        ),
        other => panic!("{other:?}"),
    }
    assert!(!path.exists(), "a refused re-baseline must write nothing");
}

#[test]
fn a_pin_from_another_reasoner_version_is_rebaseline_required() {
    let path = scratch("version").join("old.divergences");
    std::fs::write(
        &path,
        format!("# suite: {SUITE}\n# reasoner: rustdl v0.0.1-ancient\n"),
    )
    .expect("writable");
    match check_pin(&[], Path::new(SUITE), &path, false) {
        PinOutcome::RebaselineRequired(m) => {
            assert!(
                m.contains("v0.0.1-ancient") && m.contains(REASONER_VERSION),
                "{m}"
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_pin_describing_another_suite_is_rebaseline_required() {
    let path = scratch("suite").join("other.divergences");
    std::fs::write(
        &path,
        format!("# suite: suites/somewhere-else\n# reasoner: {REASONER_VERSION}\n"),
    )
    .expect("writable");
    match check_pin(&[], Path::new(SUITE), &path, false) {
        PinOutcome::RebaselineRequired(m) => {
            assert!(m.contains("somewhere-else") && m.contains(SUITE), "{m}");
        }
        other => panic!("{other:?}"),
    }
}

/// A header this cannot read is re-baseline required, not an empty
/// pin. Treating it as empty would make every real divergence
/// "unpinned" for a reason that has nothing to do with either
/// reasoner, and, on the day there are no divergences, would make an
/// unreadable file PASS.
#[test]
fn a_headerless_pin_is_rebaseline_required_not_an_empty_pin() {
    let path = scratch("headerless").join("bare.divergences");
    std::fs::write(&path, "d\tgate\tgate\tconsistent\tinconsistent\n").expect("writable");
    match check_pin(&[], Path::new(SUITE), &path, false) {
        PinOutcome::RebaselineRequired(m) => assert!(m.contains("# suite"), "{m}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn an_unparseable_row_is_rebaseline_required_not_a_dropped_row() {
    let path = scratch("badrow").join("bad.divergences");
    std::fs::write(
        &path,
        format!(
            "# suite: {SUITE}\n# reasoner: {REASONER_VERSION}\nd\tgate\tgate\tmaybe\tinconsistent\n"
        ),
    )
    .expect("writable");
    match check_pin(&[], Path::new(SUITE), &path, false) {
        PinOutcome::RebaselineRequired(m) => assert!(m.contains("maybe"), "{m}"),
        other => panic!("{other:?}"),
    }
}

// ---------------------------------------------------------------
// 4: the checked-in pin itself.
// ---------------------------------------------------------------

/// Every divergence `suites/sulo.divergences` currently records.
///
/// `timeinstant-datarange` asserts `TimeInstant subClassOf hasValue
/// only (xsd:dateTime or xsd:dateTimeStamp)`, and horned-owl drops that
/// axiom as an unsupported data range on every load, so rustdl reports
/// the case's data CONSISTENT while HermiT finds the clash the case has
/// always claimed. rustdl is the outlier and the direction is
/// incompleteness, which is harmless to soundness. It is the reason the
/// case carries `oracle-hermit` (`tests/deferred.rs`).
const EXPECTED: &[(&str, &str, &str, &str)] = &[(
    "timeinstant-datarange",
    "gate: expected inconsistent",
    "consistent",
    "inconsistent",
)];

fn checked_in() -> BTreeSet<PinnedDivergence> {
    let text =
        std::fs::read_to_string(PIN).unwrap_or_else(|e| panic!("{PIN} should be readable: {e}"));
    text.lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| PinnedDivergence::parse(l).unwrap_or_else(|e| panic!("{PIN}: {e}")))
        .collect()
}

/// The checked-in pin is diffed against `EXPECTED` in both directions,
/// in the ordinary CI job that has no Java.
///
/// Without this, `--accept-divergences` would be enough on its own to
/// absorb a change: somebody looking at a red weekly job could
/// re-baseline, commit, and the whole thing would go green with nobody
/// having reviewed what moved. The weekly job would then be pinning
/// whatever it happened to see. Editing this table is the review.
#[test]
fn the_checked_in_pin_is_exactly_the_documented_disagreement() {
    let on_disk = checked_in();
    let expected: BTreeSet<PinnedDivergence> = EXPECTED
        .iter()
        .map(|(case, check, rustdl, hermit)| PinnedDivergence {
            case_id: (*case).to_string(),
            check: (*check).to_string(),
            origin: Origin::Gate,
            rustdl: Answer::from_token(rustdl).expect("a token in EXPECTED"),
            hermit: Answer::from_token(hermit).expect("a token in EXPECTED"),
        })
        .collect();

    let added: Vec<_> = on_disk
        .difference(&expected)
        .map(PinnedDivergence::line)
        .collect();
    assert!(
        added.is_empty(),
        "{PIN} records divergence(s) this test does not know about: {added:?}. Pinning a \
         disagreement declares it expected and stops it failing CI, so it must be a \
         deliberate, reviewed act. Describe it in EXPECTED, with the reason, or drop it \
         from the pin."
    );

    let removed: Vec<_> = expected
        .difference(&on_disk)
        .map(PinnedDivergence::line)
        .collect();
    assert!(
        removed.is_empty(),
        "EXPECTED describes divergence(s) {PIN} no longer records: {removed:?}. If the gap \
         genuinely closed, say so here and in tests/deferred.rs rather than leaving a \
         stale table behind."
    );
}

/// The pin is not empty, so the diff above cannot hold vacuously by
/// comparing two empty sets.
///
/// The day the pin legitimately empties (rustdl represents the data
/// range, so the two reasoners agree everywhere), this test is the one
/// that has to be deliberately deleted, which is the point: emptying
/// the pin is a claim worth someone's attention, not a silent state.
#[test]
fn the_pin_is_not_empty() {
    assert!(
        !checked_in().is_empty(),
        "an empty {PIN} would make the both-directions diff pass against an empty table, \
         and would make the weekly differential green by describing nothing"
    );
}

/// Every case the pin names still exists in the suite.
///
/// A rename or deletion would otherwise leave the pin describing
/// nothing until the next weekly, jar-bearing run noticed. This
/// notices in the ordinary CI job, on the commit that did it.
#[test]
fn every_pinned_case_still_exists_in_the_suite() {
    let ids: BTreeSet<String> = discover(Path::new(SUITE))
        .expect("the SULO suite should be discoverable")
        .iter()
        .map(|p| {
            load_case(p)
                .unwrap_or_else(|e| panic!("{} should parse: {e}", p.display()))
                .id
        })
        .collect();

    for d in &checked_in() {
        assert!(
            ids.contains(&d.case_id),
            "{PIN} pins a divergence in case {:?}, which no longer exists under {SUITE}. A \
             pin nothing can confirm is a pin that will never fail.",
            d.case_id
        );
    }
}

/// The pin's header names this build's reasoner, so the checked-in
/// file is comparable today rather than being an instant re-baseline
/// prompt for everyone.
#[test]
fn the_checked_in_pin_names_this_reasoner_and_this_suite() {
    let text = std::fs::read_to_string(PIN).expect("readable");
    assert!(
        text.contains(&format!("# reasoner: {REASONER_VERSION}\n")),
        "{text}"
    );
    assert!(text.contains(&format!("# suite: {SUITE}\n")), "{text}");
}

// ---------------------------------------------------------------
// 5: the report must agree with the process.
// ---------------------------------------------------------------

/// The `exit_code` in the JSON payload is the code the process
/// actually returns, pin included.
///
/// Before the pin existed, that field was `differential_exit_code`,
/// which knows nothing about pinning and would report 5 on the very
/// run the process exits 0 for. A consumer reading the JSON and a
/// consumer reading `$?` would then draw opposite conclusions from one
/// run, and the JSON one would be wrong.
#[test]
fn the_json_reports_the_exit_code_the_process_will_return() {
    let asked = [diverged(
        "d",
        "gate",
        Answer::Consistent,
        Answer::Inconsistent,
    )];
    let pinned = set(&[pin("d", "gate", Answer::Consistent, Answer::Inconsistent)]);
    let d = diff(&asked, &observed(&asked), &pinned);

    let pin_path = PathBuf::from(PIN);
    let opts = DifferentialOptions {
        suite: Path::new(SUITE),
        ontology: Path::new("../sulo/sulo.ttl"),
        robot: Path::new("robot.jar"),
        filter: None,
        workdir: Path::new("probes"),
        divergences: Some(&pin_path),
        accept_divergences: false,
    };

    let text = render_json(&asked, &opts, Some(&d));
    let payload: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
    assert_eq!(
        payload["summary"]["exit_code"], 0,
        "the payload must report the pinned exit code, not the unpinned one: {text}"
    );
    assert_eq!(payload["summary"]["diverged"], 1, "{text}");
    assert_eq!(payload["pin"]["matched"][0]["case"], "d", "{text}");
    assert_eq!(
        payload["pin"]["matched"][0]["rustdl"], "consistent",
        "{text}"
    );
    assert_eq!(
        payload["pin"]["matched"][0]["hermit"], "inconsistent",
        "{text}"
    );

    // And the same payload without a pin still reports 5, so the
    // assertion above is about the pin and not about the field always
    // being 0.
    let unpinned_opts = DifferentialOptions {
        divergences: None,
        ..opts
    };
    let text = render_json(&asked, &unpinned_opts, None);
    let payload: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
    assert_eq!(payload["summary"]["exit_code"], 5, "{text}");
    assert!(payload.get("pin").is_none(), "{text}");
}
