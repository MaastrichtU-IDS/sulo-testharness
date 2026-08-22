//! `report::render`'s baseline-loss preamble (fix round 2's "cheap
//! item"): a CI consumer reading only the rendered report, not the
//! console, must still be able to see that a run carried known,
//! permanent, pinned-reasoner loss.

use sulo_testharness::report::render;
use sulo_testharness::suite::CaseResult;
use sulo_testharness::verdict::Verdict;

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
