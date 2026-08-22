use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use sulo_testharness::load::load_file;
use sulo_testharness::manifest::Case;
use sulo_testharness::suite::{downgrade_for_loss, run_case};
use sulo_testharness::verdict::{CheckOutcome, IndeterminateReason, Verdict};

fn o(v: Verdict) -> CheckOutcome {
    CheckOutcome {
        name: "c".into(),
        verdict: v,
    }
}

// ---------------------------------------------------------------
// downgrade_for_loss: the brief's own two tests, unmodified.
// ---------------------------------------------------------------

#[test]
fn loss_downgrades_all_four_untrusted_outcomes() {
    let loss = vec!["parse: 4 triples unconsumed".to_string()];
    let mut outs = vec![
        // Trustworthy under monotonicity: entailed by a subset is
        // entailed by the whole. Must survive.
        o(Verdict::Pass),
        o(Verdict::Fail(
            "expected NOT entailed, but it is entailed: x".into(),
        )),
        // Untrusted: rests on "no proof found". Must downgrade.
        o(Verdict::UnrefutedPass),
        o(Verdict::Fail(
            "expected entailed, but no proof was found: y".into(),
        )),
    ];

    downgrade_for_loss(&mut outs, &loss);

    assert_eq!(
        outs[0].verdict,
        Verdict::Pass,
        "a positive Pass stays trusted"
    );
    assert!(
        matches!(outs[1].verdict, Verdict::Fail(_)),
        "a negative Fail stays trusted"
    );
    assert!(
        matches!(
            outs[2].verdict,
            Verdict::Indeterminate(IndeterminateReason::AxiomLoss(_))
        ),
        "an unrefuted pass rests on absence of proof and must downgrade"
    );
    assert!(
        matches!(
            outs[3].verdict,
            Verdict::Indeterminate(IndeterminateReason::AxiomLoss(_))
        ),
        "a positive Fail may be a loss artifact and must downgrade"
    );
}

#[test]
fn no_loss_changes_nothing() {
    let mut outs = vec![o(Verdict::UnrefutedPass), o(Verdict::Pass)];
    let before = outs.clone();
    downgrade_for_loss(&mut outs, &[]);
    assert_eq!(outs, before);
}

// ---------------------------------------------------------------
// downgrade_for_loss: the gate's two extra untrusted shapes
// (Ruling 4, items c and d), and proof that the gate's trustworthy
// "found inconsistent" shapes are left alone in both directions.
// ---------------------------------------------------------------

#[test]
fn loss_downgrades_the_gates_missed_inconsistency() {
    // expect_inconsistent case, gate found it consistent: rests on
    // absence of a clash proof, exactly like an ordinary "no proof
    // was found" Fail.
    let loss = vec!["parse: dropped owl:AllDisjointClasses".to_string()];
    let mut outs = vec![CheckOutcome {
        name: "gate: expected inconsistent".into(),
        verdict: Verdict::Fail(
            "expected inconsistent, but the reasoner found it consistent".into(),
        ),
    }];

    downgrade_for_loss(&mut outs, &loss);

    assert!(
        matches!(
            outs[0].verdict,
            Verdict::Indeterminate(IndeterminateReason::AxiomLoss(_))
        ),
        "the gate's missed-inconsistency Fail rests on absence of a clash and must downgrade"
    );
}

#[test]
fn loss_downgrades_the_gates_found_consistent_pass() {
    // Ordinary case, gate found the ontology consistent: also rests
    // on absence of a clash, over a possibly-weakened ontology.
    let loss = vec!["parse: dropped owl:AllDisjointClasses".to_string()];
    let mut outs = vec![CheckOutcome {
        name: "gate: expected consistent".into(),
        verdict: Verdict::Pass,
    }];

    downgrade_for_loss(&mut outs, &loss);

    assert!(
        matches!(
            outs[0].verdict,
            Verdict::Indeterminate(IndeterminateReason::AxiomLoss(_))
        ),
        "the gate's found-consistent Pass rests on absence of a clash and must downgrade"
    );
}

#[test]
fn loss_does_not_downgrade_the_gates_found_inconsistent_outcomes() {
    // Finding a clash is a positive entailment: monotonically
    // trustworthy regardless of which of the two expectations it
    // appears under.
    let loss = vec!["parse: dropped owl:AllDisjointClasses".to_string()];
    let mut outs = vec![
        CheckOutcome {
            name: "gate: expected inconsistent".into(),
            verdict: Verdict::Pass,
        },
        CheckOutcome {
            name: "gate: expected consistent".into(),
            verdict: Verdict::Fail(
                "ontology plus data is inconsistent, so every entailment check \
                 below would pass vacuously. Remaining checks skipped."
                    .into(),
            ),
        },
    ];
    let before: Vec<Verdict> = outs.iter().map(|o| o.verdict.clone()).collect();

    downgrade_for_loss(&mut outs, &loss);

    let after: Vec<Verdict> = outs.iter().map(|o| o.verdict.clone()).collect();
    assert_eq!(
        before, after,
        "a gate outcome that found a clash must never be downgraded by loss"
    );
}

// ---------------------------------------------------------------
// run_case: the consistency gate's three branches, plus proof that
// a gate stop actually skips (not evaluates-and-happens-to-pass)
// the remaining checks, and that `skipped` is set per Ruling 3.
// ---------------------------------------------------------------

/// A `Case` with every field filled in with an inert default, so each
/// test only needs to override what it actually varies.
fn base_case(id: &str) -> Case {
    Case {
        id: id.into(),
        description: "test case".into(),
        ontology: None,
        imports: vec![],
        data: vec![],
        prefixes: BTreeMap::from([("ex".to_string(), "http://example.org/".to_string())]),
        expect_inconsistent: false,
        entails: None,
        not_entails: None,
        entails_manchester: vec![],
        not_entails_manchester: vec![],
        instance_of_expr: vec![],
        satisfiable_expr: vec![],
        unsatisfiable: vec![],
        tags: vec![],
        timeout_ms: 30_000,
        base_dir: PathBuf::from("tests/fixtures"),
    }
}

const UNUSED_DEFAULT: &str = "tests/fixtures/clean.ttl";

#[test]
fn gate_expect_inconsistent_and_actually_inconsistent_passes_and_skips_the_rest() {
    let mut case = base_case("gate-a");
    case.ontology = Some(PathBuf::from("inconsistent.ttl"));
    case.expect_inconsistent = true;
    // If the gate failed to stop the case, this would still be
    // evaluated and appended as a second check outcome (on an
    // inconsistent ontology it would even trivially "pass", so only
    // the check COUNT, not its result, can prove the skip happened).
    case.entails = Some("ex:x sulo:isPartOf ex:y .".into());

    let result = run_case(&case, Path::new(UNUSED_DEFAULT));

    assert_eq!(
        result.verdict,
        Verdict::Pass,
        "expected inconsistency was found"
    );
    assert!(
        result.skipped,
        "the gate stopped the case; remaining checks were skipped"
    );
    assert_eq!(
        result.checks.len(),
        1,
        "only the gate outcome should be present; the entails claim must not have run, got {:?}",
        result.checks
    );
}

#[test]
fn gate_expect_inconsistent_but_actually_consistent_fails_and_skips_the_rest() {
    let mut case = base_case("gate-b");
    case.ontology = Some(PathBuf::from("clean.ttl"));
    case.expect_inconsistent = true;
    case.entails = Some("ex:x sulo:isPartOf ex:y .".into());

    let result = run_case(&case, Path::new(UNUSED_DEFAULT));

    assert!(
        matches!(result.verdict, Verdict::Fail(_)),
        "the expected clash never fired"
    );
    assert!(
        result.skipped,
        "the gate stopped the case; remaining checks were skipped"
    );
    assert_eq!(
        result.checks.len(),
        1,
        "only the gate outcome should be present; the entails claim must not have run, got {:?}",
        result.checks
    );
    if let Verdict::Fail(msg) = &result.verdict {
        assert!(
            msg.contains("consistent"),
            "the failure should say the reasoner found it consistent, got: {msg}"
        );
    }
}

#[test]
fn gate_expects_consistent_but_ontology_is_inconsistent_fails_and_skips_the_rest() {
    let mut case = base_case("gate-c");
    case.ontology = Some(PathBuf::from("inconsistent.ttl"));
    // expect_inconsistent defaults to false in base_case.
    case.entails = Some("ex:x sulo:isPartOf ex:y .".into());

    let result = run_case(&case, Path::new(UNUSED_DEFAULT));

    assert!(
        matches!(result.verdict, Verdict::Fail(_)),
        "an unexpected inconsistency must fail"
    );
    assert!(
        result.skipped,
        "the gate stopped the case; remaining checks were skipped"
    );
    assert_eq!(
        result.checks.len(),
        1,
        "only the gate outcome should be present; the entails claim must not have run, got {:?}",
        result.checks
    );
    if let Verdict::Fail(msg) = &result.verdict {
        assert!(
            msg.contains("vacuously"),
            "the message should warn that every check below would have passed vacuously, got: {msg}"
        );
    }
}

#[test]
fn gate_expects_and_gets_consistency_and_runs_the_rest_of_the_case() {
    let mut case = base_case("gate-d");
    case.ontology = Some(PathBuf::from("clean.ttl"));
    // A trivially true claim over the fixture ontology: ex:B is
    // asserted rdfs:subClassOf ex:A.
    case.entails = Some("ex:B rdfs:subClassOf ex:A .".into());

    let result = run_case(&case, Path::new(UNUSED_DEFAULT));

    assert!(
        !result.skipped,
        "the gate passed cleanly; the rest of the case must run"
    );
    assert_eq!(
        result.checks.len(),
        2,
        "the gate outcome plus the one entails check should both be present, got {:?}",
        result.checks
    );
    assert_eq!(result.verdict, Verdict::Pass);
}

// ---------------------------------------------------------------
// The property-declaration gap from Task 6: a claim whose predicate
// is declared as something other than the kind of property the
// claim's shape implies must surface as Indeterminate(OracleError)
// with the ontology-declaration mismatch named in the message, never
// as a silent green.
// ---------------------------------------------------------------

#[test]
fn a_claim_against_a_mis_declared_predicate_is_indeterminate_not_a_silent_pass() {
    let mut case = base_case("annotation-mismatch");
    case.ontology = Some(PathBuf::from("annotation-only.ttl"));
    // rdfs:label is declared owl:AnnotationProperty in the fixture,
    // so this classifies as a DataPropertyAssertion but is not
    // actually a data property: a configuration mistake, not a
    // reasoner question.
    case.entails = Some("ex:x rdfs:label \"hello\" .".into());

    let result = run_case(&case, Path::new(UNUSED_DEFAULT));

    let bad = result
        .checks
        .iter()
        .find(|c| {
            matches!(
                c.verdict,
                Verdict::Indeterminate(IndeterminateReason::OracleError(_))
            )
        })
        .unwrap_or_else(|| {
            panic!(
                "expected an Indeterminate(OracleError), got {:?}",
                result.checks
            )
        });

    if let Verdict::Indeterminate(IndeterminateReason::OracleError(msg)) = &bad.verdict {
        assert!(
            msg.contains("annotation property") && msg.contains("not a data property"),
            "the message should name the declaration mismatch, got: {msg}"
        );
    }
    assert!(
        matches!(result.verdict, Verdict::Indeterminate(_)),
        "a manifest predicate mistake must never resolve to a trustworthy Pass or Fail"
    );
}

// ---------------------------------------------------------------
// timeout_ms wiring (fix round 1): the deadline a case declares must
// actually reach the oracle. Same case, same claim, only the budget
// differs: a tiny budget must produce Indeterminate(Timeout); a
// generous one must produce a real verdict instead.
// ---------------------------------------------------------------

const SULO: &str = "../sulo/sulo.ttl";

/// A language-tagged DataPropertyAssertion claim, identical in shape
/// to `tests/oracle.rs`'s `a_zero_deadline_yields_timeout_not_a_false_negative`,
/// which already proves a zero deadline reliably times this exact
/// call out against SULO plus `parts.ttl`. Per `oracle`'s own module
/// doc, the materialised fast path drops language tags entirely, so
/// this claim always routes to the deadline-bounded
/// `entailed_via_satisfiability_probe` fallback: a real tableau call
/// every time, never short-circuited by a cheap structural match.
/// A minimal, loss-free fixture was tried first and rejected: the
/// search space was too trivial for a zero deadline to ever be
/// noticed, so the reasoner returned a definite answer before the
/// cooperative deadline check ever fired. Real SULO's size is what
/// makes the zero-deadline case genuinely bite.
fn timeout_sensitive_case(timeout_ms: u64) -> Case {
    let mut case = base_case("timeout-wiring");
    case.data = vec![PathBuf::from("parts.ttl")];
    case.entails = Some("ex:n sulo:hasValue \"bonjour\"@fr .".into());
    case.timeout_ms = timeout_ms;
    case
}

#[test]
fn a_tiny_timeout_ms_yields_indeterminate_timeout() {
    let case = timeout_sensitive_case(0);
    let result = run_case(&case, Path::new(SULO));

    let entails_check = result
        .checks
        .iter()
        .find(|c| !c.name.starts_with("gate:"))
        .unwrap_or_else(|| panic!("expected the entails check to run, got {:?}", result.checks));

    assert!(
        matches!(
            entails_check.verdict,
            Verdict::Indeterminate(IndeterminateReason::Timeout)
        ),
        "a timeout_ms of 0 must expire immediately, got {:?}",
        entails_check.verdict
    );
}

#[test]
fn a_generous_timeout_ms_yields_a_real_verdict_not_a_timeout() {
    let case = timeout_sensitive_case(30_000);
    let result = run_case(&case, Path::new(SULO));

    let entails_check = result
        .checks
        .iter()
        .find(|c| !c.name.starts_with("gate:"))
        .unwrap_or_else(|| panic!("expected the entails check to run, got {:?}", result.checks));

    assert!(
        !matches!(
            entails_check.verdict,
            Verdict::Indeterminate(IndeterminateReason::Timeout)
        ),
        "a generous timeout_ms must not expire, got {:?}",
        entails_check.verdict
    );
    // Real verdict, not vacuous, and distinct in kind from the tiny
    // budget's Timeout: the reasoner actually finished its search
    // this time. This exact claim is a documented reasoner-
    // completeness gap for language-tagged data values (see
    // data_fallback_terminates_with_a_sound_answer_for_a_language_tagged_literal
    // in tests/oracle.rs), so it resolves to a trustworthy "no proof
    // was found" Fail. That Fail is then downgraded to
    // Indeterminate(AxiomLoss(_)) per Ruling 4, item (a), but NOT by
    // real SULO's own dropped-data-range conversion loss: that is now
    // a recognised, permanent baseline (src/load.rs's
    // KNOWN_BASELINE_KIND) and no longer downgrades anything. The
    // downgrade here instead comes from THIS FIXTURE's own,
    // independent, genuine drop: tests/fixtures/parts.ttl's three
    // `sulo:hasValue` literal assertions (plain, language-tagged,
    // typed) include two rustdl's IR cannot represent
    // ("DataPropertyAssertion: unsupported data range", loaded and
    // reported before this fixture is even merged with SULO), which
    // does not match the SubClassOf-shaped baseline and so is not
    // exempted. The downgrade firing is itself proof the reasoner
    // completed rather than expiring.
    assert!(
        matches!(
            &entails_check.verdict,
            Verdict::Indeterminate(IndeterminateReason::AxiomLoss(_))
        ),
        "expected a completed-and-downgraded verdict distinct from Timeout, got {:?}",
        entails_check.verdict
    );
}

// ---------------------------------------------------------------
// Known-baseline loss allowlist (fix round 1, Ruling 1): loss that
// matches the pinned reasoner's one known, permanent,
// SubClassOf-shaped data-range gap must not downgrade anything; loss
// that does not match it (a different fixture's genuinely different
// drop) must downgrade exactly as before. Both directions proven
// against real `load_file` output, not a synthetic loss string, so
// this actually exercises the allowlist comparison in src/load.rs.
// ---------------------------------------------------------------

#[test]
fn baseline_only_loss_from_real_sulo_does_not_downgrade() {
    let loaded = load_file(Path::new(SULO)).expect("real SULO should load");

    assert!(
        loaded.loss.is_empty(),
        "real SULO's only known loss is the baseline; loss beyond it should be empty, got {:?}",
        loaded.loss
    );
    assert!(
        !loaded.baseline_loss.is_empty(),
        "the known baseline drop should still be surfaced, just not as loss"
    );

    let mut outs = vec![o(Verdict::Fail(
        "expected to hold, but no proof was found: x".into(),
    ))];
    downgrade_for_loss(&mut outs, &loaded.loss);

    assert!(
        matches!(outs[0].verdict, Verdict::Fail(_)),
        "baseline-only loss must not downgrade a positive Fail, got {:?}",
        outs[0].verdict
    );
}

#[test]
fn loss_beyond_the_baseline_still_downgrades() {
    // tests/fixtures/parts.ttl carries a genuinely different drop
    // (DataPropertyAssertion, not SubClassOf) from three sulo:hasValue
    // literal assertions rustdl's IR partly cannot represent. It does
    // not match the SubClassOf-shaped baseline, so it must land in
    // `loss`, not `baseline_loss`.
    let loaded = load_file(Path::new("tests/fixtures/parts.ttl")).expect("fixture should load");

    assert!(
        !loaded.loss.is_empty(),
        "a non-baseline-shaped drop must still be reported as loss"
    );
    assert!(
        loaded.baseline_loss.is_empty(),
        "a drop that does not match the baseline exactly must not be folded into it, got {:?}",
        loaded.baseline_loss
    );

    let mut outs = vec![o(Verdict::Fail(
        "expected to hold, but no proof was found: x".into(),
    ))];
    downgrade_for_loss(&mut outs, &loaded.loss);

    assert!(
        matches!(
            outs[0].verdict,
            Verdict::Indeterminate(IndeterminateReason::AxiomLoss(_))
        ),
        "loss beyond the baseline must still downgrade a positive Fail, got {:?}",
        outs[0].verdict
    );
}
