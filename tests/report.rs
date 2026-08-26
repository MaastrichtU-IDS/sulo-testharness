//! `report::render`, the surface where the harness's honesty is
//! actually delivered to a reader.
//!
//! Two groups of tests. The first is the baseline-loss preamble (fix
//! round 2's "cheap item"): a CI consumer reading only the rendered
//! report, not the console, must still be able to see that a run
//! carried known, permanent, pinned-reasoner loss. The second covers
//! what spec 5.1 actually requires of the output and what was
//! previously untested altogether: unrefuted passes counted and
//! reported SEPARATELY from verified passes, the four-way verdict tag
//! mapping, the skip notice, and the per-check Fail and Indeterminate
//! message lines. Before these, deleting the `unrefuted += 1`
//! increment or collapsing the `PASS*` tag to `PASS` left every test
//! in the repository green.

use sulo_testharness::report::render;
use sulo_testharness::suite::CaseResult;
use sulo_testharness::verdict::{CheckOutcome, IndeterminateReason, Verdict};

fn passing_case(id: &str, baseline_loss: Vec<String>) -> CaseResult {
    CaseResult {
        id: id.into(),
        verdict: Verdict::Pass,
        checks: vec![],
        skipped: false,
        baseline_loss,
    }
}

#[test]
fn baseline_loss_appears_as_a_report_preamble() {
    let results = vec![passing_case(
        "c1",
        vec!["conversion: 2 dropped (SubClassOf: unsupported data range x2)".into()],
    )];

    let out = render(&results);

    assert!(
        out.contains("known baseline loss"),
        "expected a baseline-loss preamble, got: {out}"
    );
    assert!(
        out.contains("SubClassOf: unsupported data range x2"),
        "expected the actual baseline message to appear verbatim, got: {out}"
    );
}

#[test]
fn no_baseline_loss_means_no_preamble() {
    let results = vec![passing_case("c1", vec![])];

    let out = render(&results);

    assert!(
        !out.contains("known baseline loss"),
        "no case carried baseline loss, so no preamble should appear, got: {out}"
    );
}

#[test]
fn baseline_loss_is_deduplicated_across_cases() {
    let msg = "conversion: 2 dropped (SubClassOf: unsupported data range x2)".to_string();
    let results = vec![
        passing_case("c1", vec![msg.clone()]),
        passing_case("c2", vec![msg.clone()]),
    ];

    let out = render(&results);
    let occurrences = out.matches("SubClassOf: unsupported data range x2").count();
    assert_eq!(
        occurrences, 1,
        "the same baseline message from two cases should be deduplicated, got: {out}"
    );
}

fn case_with(id: &str, verdict: Verdict, checks: Vec<CheckOutcome>, skipped: bool) -> CaseResult {
    CaseResult {
        id: id.into(),
        verdict,
        checks,
        skipped,
        baseline_loss: vec![],
    }
}

fn check(name: &str, verdict: Verdict) -> CheckOutcome {
    CheckOutcome {
        name: name.into(),
        verdict,
        rests_on_absence: false,
    }
}

#[test]
fn unrefuted_passes_are_counted_and_reported_separately() {
    // Spec 5.1: a negative expectation the reasoner failed to refute
    // is not a verified pass, and the report must not let a reader
    // mistake one for the other. Two unrefuted checks across two
    // cases, plus one verified Pass that must NOT be counted.
    let results = vec![
        case_with(
            "c1",
            Verdict::UnrefutedPass,
            vec![
                check("neg", Verdict::UnrefutedPass),
                check("pos", Verdict::Pass),
            ],
            false,
        ),
        case_with(
            "c2",
            Verdict::UnrefutedPass,
            vec![check("neg", Verdict::UnrefutedPass)],
            false,
        ),
    ];

    let out = render(&results);

    assert!(
        out.contains("2 check(s) marked PASS*"),
        "both unrefuted checks must be counted, and the verified Pass must not \
         be counted among them, got: {out}"
    );
    assert!(
        out.contains("not a proof of non-entailment"),
        "the summary must say what PASS* means, got: {out}"
    );
    assert!(
        out.contains("PASS*  c1") && out.contains("PASS*  c2"),
        "an unrefuted case must be tagged PASS*, distinctly from PASS, got: {out}"
    );
}

#[test]
fn no_unrefuted_checks_means_no_unrefuted_summary() {
    let results = vec![case_with(
        "c1",
        Verdict::Pass,
        vec![check("pos", Verdict::Pass)],
        false,
    )];
    let out = render(&results);
    assert!(
        !out.contains("PASS*"),
        "with nothing unrefuted, neither the tag nor the summary should appear, got: {out}"
    );
}

#[test]
fn the_four_verdicts_map_to_four_distinct_tags() {
    // A four-way mapping collapsed to three (the likeliest edit) would
    // erase exactly the distinction the harness exists to make.
    let results = vec![
        case_with("pass-case", Verdict::Pass, vec![], false),
        case_with("unrefuted-case", Verdict::UnrefutedPass, vec![], false),
        case_with(
            "indet-case",
            Verdict::Indeterminate(IndeterminateReason::Timeout),
            vec![],
            false,
        ),
        case_with("fail-case", Verdict::Fail("boom".into()), vec![], false),
    ];

    let out = render(&results);
    let tags: Vec<&str> = out
        .lines()
        .filter_map(|l| l.split_whitespace().next())
        .collect();

    assert_eq!(
        tags,
        vec!["PASS", "PASS*", "INDET", "FAIL"],
        "each verdict must render as its own tag, got: {out}"
    );
}

#[test]
fn fail_and_indeterminate_check_messages_and_the_skip_notice_are_rendered() {
    // A Fail whose reason never reaches the reader is a red build with
    // no diagnosis; a skipped case that looks like a completed one is
    // worse, because the checks below the gate never ran at all.
    let results = vec![case_with(
        "gate-stopped",
        Verdict::Fail("gate said no".into()),
        vec![
            check(
                "gate: expected consistent",
                Verdict::Fail("ontology plus data is inconsistent".into()),
            ),
            check(
                "later",
                Verdict::Indeterminate(IndeterminateReason::AxiomLoss(
                    "parse: dropped owl:AllDisjointClasses".into(),
                )),
            ),
        ],
        true,
    )];

    let out = render(&results);

    assert!(
        out.contains("ontology plus data is inconsistent"),
        "a failing check's message must be rendered verbatim, got: {out}"
    );
    assert!(
        out.contains("indeterminate:") && out.contains("AxiomLoss"),
        "an indeterminate check must render its reason, got: {out}"
    );
    assert!(
        out.contains("remaining checks skipped (see gate)"),
        "a skipped case must say so, or its unrun checks read as passes, got: {out}"
    );
}
