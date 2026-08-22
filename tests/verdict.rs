use sulo_testharness::verdict::{CheckOutcome, IndeterminateReason, Verdict, aggregate, exit_code};

fn outcome(name: &str, v: Verdict) -> CheckOutcome {
    CheckOutcome {
        name: name.to_string(),
        verdict: v,
    }
}

#[test]
fn empty_suite_passes() {
    assert_eq!(aggregate(&[]), Verdict::Pass);
}

#[test]
fn fail_beats_everything() {
    let out = vec![
        outcome("a", Verdict::Pass),
        outcome("b", Verdict::UnrefutedPass),
        outcome("c", Verdict::Indeterminate(IndeterminateReason::Timeout)),
        outcome("d", Verdict::Fail("boom".into())),
    ];
    assert!(matches!(aggregate(&out), Verdict::Fail(_)));
}

#[test]
fn indeterminate_beats_unrefuted_pass() {
    let out = vec![
        outcome("a", Verdict::UnrefutedPass),
        outcome("b", Verdict::Indeterminate(IndeterminateReason::Timeout)),
    ];
    assert!(matches!(aggregate(&out), Verdict::Indeterminate(_)));
}

#[test]
fn unrefuted_pass_beats_pass() {
    let out = vec![
        outcome("a", Verdict::Pass),
        outcome("b", Verdict::UnrefutedPass),
    ];
    assert_eq!(aggregate(&out), Verdict::UnrefutedPass);
}

#[test]
fn exit_codes_match_the_contract() {
    assert_eq!(exit_code(&Verdict::Pass), 0);
    assert_eq!(exit_code(&Verdict::UnrefutedPass), 0);
    assert_eq!(exit_code(&Verdict::Fail("x".into())), 1);
    assert_eq!(
        exit_code(&Verdict::Indeterminate(IndeterminateReason::Timeout)),
        3
    );
}
