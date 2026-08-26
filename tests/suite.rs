use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use sulo_testharness::load::load_file;
use sulo_testharness::manifest::{Case, CqSpec, load_case};
use sulo_testharness::suite::{downgrade_for_loss, run_case};
use sulo_testharness::verdict::{CheckOutcome, IndeterminateReason, Verdict};

fn o(v: Verdict) -> CheckOutcome {
    CheckOutcome {
        name: "c".into(),
        verdict: v,
        rests_on_absence: false,
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
        rests_on_absence: false,
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
        rests_on_absence: false,
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
            rests_on_absence: false,
        },
        CheckOutcome {
            name: "gate: expected consistent".into(),
            verdict: Verdict::Fail(
                "ontology plus data is inconsistent, so every entailment check \
                 below would pass vacuously. Remaining checks skipped."
                    .into(),
            ),
            rests_on_absence: false,
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
// downgrade_for_loss: the competency-question shapes, which carry no
// `oracle::NO_PROOF_MARKER` and no `GATE_*` name and so are matched
// through `CheckOutcome::rests_on_absence` instead. Both polarities,
// because over-downgrading is its own defect: a CQ Pass with
// `exact: false` and every cell bound asserts only that certain rows
// are PRESENT, which loss (a strict subset of the axioms, hence a
// subset of the closure) can never manufacture.
// ---------------------------------------------------------------

/// A `CheckOutcome` shaped exactly like one `cq::check_cq` builds:
/// a `cq <query>` name, and the `rests_on_absence` flag `check_cq`
/// would have computed for that verdict and spec.
fn cq_outcome(verdict: Verdict, rests_on_absence: bool) -> CheckOutcome {
    CheckOutcome {
        name: "cq queries/who-participated.rq".into(),
        verdict,
        rests_on_absence,
    }
}

#[test]
fn loss_downgrades_a_cq_fail_and_an_absence_claiming_cq_pass() {
    let loss = vec!["parse: 4 triples unconsumed".to_string()];
    let mut outs = vec![
        // A CQ Fail. Built by `rows::compare`, so its message carries
        // no NO_PROOF_MARKER: without the flag this reads as a
        // trustworthy ontology regression, when the row may simply
        // have been suppressed by the dropped axiom.
        cq_outcome(
            Verdict::Fail("missing expected row: {?p = <http://example.org/alice>}".into()),
            true,
        ),
        // A CQ Pass whose spec said `exact: true`, an "and no other
        // rows" claim. Loss shrinks the closure, so a suppressed
        // extra row makes that claim pass unearned.
        cq_outcome(Verdict::Pass, true),
    ];

    downgrade_for_loss(&mut outs, &loss);

    assert!(
        matches!(
            outs[0].verdict,
            Verdict::Indeterminate(IndeterminateReason::AxiomLoss(_))
        ),
        "a CQ Fail may be a loss artifact and must downgrade, got {:?}",
        outs[0].verdict
    );
    assert!(
        matches!(
            outs[1].verdict,
            Verdict::Indeterminate(IndeterminateReason::AxiomLoss(_))
        ),
        "a CQ Pass making an absence claim must downgrade, got {:?}",
        outs[1].verdict
    );
}

#[test]
fn loss_does_not_downgrade_a_monotone_safe_cq_pass() {
    // `exact: false` with every expected cell bound, over a MONOTONE
    // query, asserts only presence, which axiom loss cannot fake in
    // the passing direction. Downgrading it would overstate the
    // harness's uncertainty, the mirror image of the defect above.
    // The monotonicity condition is not decided here: `check_cq`
    // owns it (see `cq::query_is_monotone`, and the LIMIT and
    // aggregate tests in `tests/cq.rs`), and this function sees only
    // the flag it produced.
    let loss = vec!["parse: 4 triples unconsumed".to_string()];
    let mut outs = vec![cq_outcome(Verdict::Pass, false)];

    downgrade_for_loss(&mut outs, &loss);

    assert_eq!(
        outs[0].verdict,
        Verdict::Pass,
        "a subset CQ Pass with no unbound cell is monotone-safe and must stay Pass"
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
        cq: vec![],
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

// ---------------------------------------------------------------
// Every wired manifest field, end to end through `run_case`. Six of
// them (imports, not_entails, not_entails_manchester,
// instance_of_expr, satisfiable_expr, unsatisfiable) were reachable
// only in principle before this: flipping
// `not_entails_manchester`'s Expectation::NotEntailed to Entailed, or
// reverting the `unsatisfiable` prefix-expansion error branch, broke
// no test at all. Asserting PER CHECK rather than on the aggregate is
// what makes a single field's polarity error visible.
// ---------------------------------------------------------------

fn check_named<'a>(result: &'a sulo_testharness::suite::CaseResult, needle: &str) -> &'a Verdict {
    &result
        .checks
        .iter()
        .find(|c| c.name.contains(needle))
        .unwrap_or_else(|| {
            panic!(
                "no check whose name contains {needle:?}; checks were {:?}",
                result.checks
            )
        })
        .verdict
}

#[test]
fn every_wired_manifest_field_is_exercised_end_to_end() {
    let case = load_case(Path::new("tests/fixtures/case-all-fields.yaml"))
        .expect("the all-fields fixture should parse");

    // `data:` as a LIST: OneOrMany::Many, previously untested.
    assert_eq!(case.data.len(), 2, "data: given as a list of two files");
    assert_eq!(case.imports.len(), 1, "imports: is populated");

    let result = run_case(&case, Path::new(UNUSED_DEFAULT));
    assert!(
        !result.skipped,
        "the gate must pass so every field below actually runs, got {:?}",
        result.checks
    );
    assert!(
        result.baseline_loss.is_empty() && result.checks.len() >= 8,
        "expected the gate plus one check per assertion, got {:?}",
        result.checks
    );

    // entails and not_entails both classify as ClassAssertion claims.
    // not_entails: ex:i2 is a C, and C is disjoint from A, so "i2 is
    // an A" is not entailed. Unrefuted, never Pass.
    let class_assertions: Vec<&Verdict> = result
        .checks
        .iter()
        .filter(|c| c.name.contains("ClassAssertion"))
        .map(|c| &c.verdict)
        .collect();
    assert_eq!(class_assertions.len(), 2, "entails plus not_entails");
    assert!(
        class_assertions.contains(&&Verdict::Pass)
            && class_assertions.contains(&&Verdict::UnrefutedPass),
        "the positive must be a trustworthy Pass and the negative only \
         UnrefutedPass, got {class_assertions:?}"
    );

    // entails_manchester: B subsumed by A, provable.
    assert_eq!(check_named(&result, "ex:B subClassOf ex:A"), &Verdict::Pass);

    // not_entails_manchester: A is NOT subsumed by B. This is the
    // check that catches a verdict-polarity flip: with
    // Expectation::Entailed it would be a Fail instead.
    assert_eq!(
        check_named(&result, "ex:A subClassOf ex:B"),
        &Verdict::UnrefutedPass,
        "a negative subsumption expectation the reasoner failed to refute"
    );

    // instance_of_expr: i1 is an A (via B) that p-relates to i2, a C.
    // Needs BOTH data files, so this also proves the list loaded.
    assert_eq!(
        check_named(&result, "instanceOf"),
        &Verdict::Pass,
        "membership in the composite expression is provable"
    );

    // satisfiable_expr: satisfiable, which is this probe's unprovable
    // direction, hence UnrefutedPass. See check_satisfiable_expr.
    assert_eq!(
        check_named(&result, "satisfiable: ex:A"),
        &Verdict::UnrefutedPass
    );

    // unsatisfiable: ex:Bad exists only in the imported file, so a Pass
    // here is proof the imports channel loaded and merged.
    assert_eq!(
        check_named(&result, "Unsatisfiable"),
        &Verdict::Pass,
        "ex:Bad is under two disjoint classes; it lives only in the import"
    );

    // unsatisfiable with an unbound prefix: surfaced as the PREFIX
    // mistake it is, never silently retried against the raw,
    // unexpanded token. Asserting on the message, not just the verdict
    // kind: handing `nosuch:Whatever` to the reasoner verbatim also
    // yields Indeterminate(OracleError), just with an unhelpful
    // UnknownClass inside it, so only the message discriminates.
    let bad_prefix = check_named(&result, "unsatisfiable: nosuch:Whatever");
    let Verdict::Indeterminate(IndeterminateReason::OracleError(msg)) = bad_prefix else {
        panic!("an unbound prefix is a configuration error, got {bad_prefix:?}");
    };
    assert!(
        msg.contains("is not bound"),
        "the error must name the unbound prefix rather than report a reasoner \
         UnknownClass for a token the author never wrote, got: {msg}"
    );
}

// ---------------------------------------------------------------
// A fragment that parses to zero claims. Valid Turtle, no triples,
// so the old code pushed no checks and `aggregate` returned Pass over
// an empty set: the second route to a case that asserts nothing and
// reports green.
// ---------------------------------------------------------------

#[test]
fn an_entails_fragment_with_no_triples_is_indeterminate_not_a_pass() {
    let mut case = base_case("empty-fragment");
    case.ontology = Some(PathBuf::from("clean.ttl"));
    case.entails = Some("  \n# only a comment\n  ".into());

    let result = run_case(&case, Path::new(UNUSED_DEFAULT));

    let empty = check_named(&result, "empty fragment");
    assert!(
        matches!(
            empty,
            Verdict::Indeterminate(IndeterminateReason::OracleError(_))
        ),
        "an empty fragment must be surfaced, got {empty:?}"
    );
    if let Verdict::Indeterminate(IndeterminateReason::OracleError(msg)) = empty {
        assert!(
            msg.contains("entails") && msg.contains("zero claims"),
            "the message should name the field and say why, got: {msg}"
        );
    }
    assert!(
        matches!(result.verdict, Verdict::Indeterminate(_)),
        "the case must not aggregate to Pass, got {:?}",
        result.verdict
    );
}

#[test]
fn a_not_entails_fragment_with_no_triples_is_indeterminate_too() {
    let mut case = base_case("empty-negative-fragment");
    case.ontology = Some(PathBuf::from("clean.ttl"));
    case.not_entails = Some("\n".into());

    let result = run_case(&case, Path::new(UNUSED_DEFAULT));

    if let Verdict::Indeterminate(IndeterminateReason::OracleError(msg)) =
        check_named(&result, "empty fragment")
    {
        assert!(
            msg.contains("not_entails"),
            "the message should name the not_entails field, got: {msg}"
        );
    } else {
        panic!(
            "expected Indeterminate(OracleError), got {:?}",
            result.checks
        );
    }
}

// ---------------------------------------------------------------
// Task 5: wiring `cq:` into `run_case`. Four load-bearing rules,
// tested in the order the task brief states them:
//
// 1. materialise ONCE per case, not once per competency question
//    (`cq_two_specs_on_the_same_case_runs_both` proves both entries
//    still run correctly and independently, which a per-question
//    materialiser would also satisfy: a call count is not observable
//    from outside `suite.rs`, so the COST claim itself is recorded as
//    a wall-clock note in the task report, not a unit test);
// 2. a gate stop must run zero CQ checks;
// 3. a `MaterializeError` makes every CQ Indeterminate, not Fail;
// 4. the deadline is `case.timeout_ms`, threaded exactly like every
//    other check.
//
// `clean.ttl` (ex:A, ex:B rdfs:subClassOf ex:A, no individuals) is
// deliberately small: `queries/subclass_of.rq` selects every
// `?s rdfs:subClassOf ?o` pair, and the materialised closure over
// this fixture contains exactly one such pair (no reasoner-inferred
// subClassOf triples are added; step 2 of `materialize` only adds
// rdf:type instance triples), so the expected row set is a single,
// fully deterministic row: `{s: ex:B, o: ex:A}`.
// ---------------------------------------------------------------

fn cq_row(pairs: &[(&str, &str)]) -> BTreeMap<String, Option<String>> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), Some((*v).to_string())))
        .collect()
}

fn subclass_of_spec(expect_rows: Vec<BTreeMap<String, Option<String>>>) -> CqSpec {
    CqSpec {
        query: PathBuf::from("queries/subclass_of.rq"),
        expect_rows,
        exact: true,
        ordered: false,
    }
}

fn classes_spec(expect_rows: Vec<BTreeMap<String, Option<String>>>) -> CqSpec {
    CqSpec {
        query: PathBuf::from("queries/classes.rq"),
        expect_rows,
        exact: true,
        ordered: false,
    }
}

#[test]
fn cq_two_specs_on_the_same_case_runs_both() {
    // Two DISTINCT queries, so each check gets its own name and
    // cannot be confused with the other. One deliberately matches
    // (Pass), the other deliberately does not (Fail): the strongest
    // form of this test, since a bug that reused the first spec for
    // every loop iteration (or dropped the second entirely) would
    // either produce the wrong verdict under the wrong name or make
    // one of the two `check_named` lookups panic outright.
    let mut case = base_case("cq-two-specs");
    case.ontology = Some(PathBuf::from("clean.ttl"));
    case.cq = vec![
        // subclass_of.rq: the one true row, s: ex:B, o: ex:A. Passes.
        subclass_of_spec(vec![cq_row(&[("s", "ex:B"), ("o", "ex:A")])]),
        // classes.rq: clean.ttl declares TWO classes (ex:A, ex:B),
        // but this expects only ex:A under exact: true. Fails.
        classes_spec(vec![cq_row(&[("c", "ex:A")])]),
    ];

    let result = run_case(&case, Path::new(UNUSED_DEFAULT));

    assert_eq!(
        result.checks.len(),
        3,
        "gate plus both cq checks should be present, got {:?}",
        result.checks
    );
    assert_eq!(
        check_named(&result, "cq queries/subclass_of.rq"),
        &Verdict::Pass,
        "the first spec's own check must pass on its own terms"
    );
    let second = check_named(&result, "cq queries/classes.rq");
    assert!(
        matches!(second, Verdict::Fail(_)),
        "the second spec's own check must fail on its own terms, got {second:?}"
    );
    assert!(
        matches!(result.verdict, Verdict::Fail(_)),
        "one failing cq must fail the whole case, got {:?}",
        result.verdict
    );
}

#[test]
fn a_matching_cq_yields_an_overall_pass() {
    let mut case = base_case("cq-match");
    case.ontology = Some(PathBuf::from("clean.ttl"));
    case.cq = vec![subclass_of_spec(vec![cq_row(&[
        ("s", "ex:B"),
        ("o", "ex:A"),
    ])])];

    let result = run_case(&case, Path::new(UNUSED_DEFAULT));

    assert_eq!(
        result.checks.len(),
        2,
        "gate plus the one cq check should both be present, got {:?}",
        result.checks
    );
    assert_eq!(result.verdict, Verdict::Pass, "got {:?}", result.checks);
}

#[test]
fn a_mismatched_cq_yields_an_overall_fail() {
    let mut case = base_case("cq-mismatch");
    case.ontology = Some(PathBuf::from("clean.ttl"));
    // ex:A is never a subject of rdfs:subClassOf in this fixture, so
    // this expected row can never match the real answer.
    case.cq = vec![subclass_of_spec(vec![cq_row(&[
        ("s", "ex:A"),
        ("o", "ex:B"),
    ])])];

    let result = run_case(&case, Path::new(UNUSED_DEFAULT));

    assert_eq!(
        result.checks.len(),
        2,
        "gate plus the one cq check should both be present, got {:?}",
        result.checks
    );
    assert!(
        matches!(result.verdict, Verdict::Fail(_)),
        "a mismatched cq row must fail the case, got {:?}",
        result.verdict
    );
    let cq_check = check_named(&result, "cq ");
    assert!(
        matches!(cq_check, Verdict::Fail(_)),
        "the cq check itself must be the Fail, got {cq_check:?}"
    );
}

#[test]
fn a_case_with_both_entails_and_cq_runs_both() {
    let mut case = base_case("cq-plus-entails");
    case.ontology = Some(PathBuf::from("clean.ttl"));
    case.entails = Some("ex:B rdfs:subClassOf ex:A .".into());
    case.cq = vec![subclass_of_spec(vec![cq_row(&[
        ("s", "ex:B"),
        ("o", "ex:A"),
    ])])];

    let result = run_case(&case, Path::new(UNUSED_DEFAULT));

    assert_eq!(
        result.checks.len(),
        3,
        "gate, entails check, and cq check must all three be present, got {:?}",
        result.checks
    );
    assert_eq!(result.verdict, Verdict::Pass, "got {:?}", result.checks);
    assert_eq!(check_named(&result, "cq "), &Verdict::Pass);
    assert_eq!(
        check_named(&result, "Subsumption"),
        &Verdict::Pass,
        "the entails claim (ex:B rdfs:subClassOf ex:A) must also have run and passed"
    );
}

#[test]
fn a_gate_stop_runs_zero_cq_checks() {
    let mut case = base_case("cq-gate-stop");
    case.ontology = Some(PathBuf::from("inconsistent.ttl"));
    case.expect_inconsistent = true;
    // If materialisation or the cq loop ran anyway, this checks.len()
    // assertion would catch it even if the cq check happened to
    // "pass" vacuously against an inconsistent closure: the same
    // reasoning the pre-existing gate tests use for entails.
    case.cq = vec![subclass_of_spec(vec![cq_row(&[
        ("s", "ex:B"),
        ("o", "ex:A"),
    ])])];

    let result = run_case(&case, Path::new(UNUSED_DEFAULT));

    assert!(
        result.skipped,
        "the gate stopped the case; the cq check must have been skipped"
    );
    assert_eq!(
        result.checks.len(),
        1,
        "only the gate outcome should be present; the cq check must not have run, got {:?}",
        result.checks
    );
}

#[test]
fn a_materialize_error_makes_every_cq_indeterminate_not_fail() {
    // timeout_ms: 0 forces materialize's very first deadline check to
    // fail immediately (the same zero-deadline seam
    // oracle::holds_with_deadline already uses), so the store is never
    // built. The gate itself is unbounded and ignores timeout_ms, so
    // it still runs and passes; only the cq path is affected.
    let mut case = base_case("cq-materialize-error");
    case.ontology = Some(PathBuf::from("clean.ttl"));
    case.timeout_ms = 0;
    case.cq = vec![subclass_of_spec(vec![cq_row(&[
        ("s", "ex:B"),
        ("o", "ex:A"),
    ])])];

    let result = run_case(&case, Path::new(UNUSED_DEFAULT));

    assert_eq!(
        result.checks.len(),
        2,
        "gate plus the one cq check should both be present, got {:?}",
        result.checks
    );
    let cq_check = check_named(&result, "cq ");
    match cq_check {
        Verdict::Indeterminate(IndeterminateReason::OracleError(msg)) => {
            assert!(
                msg.contains("time budget"),
                "the reason should carry the MaterializeError text, got: {msg}"
            );
        }
        other => panic!(
            "a MaterializeError must make the cq check Indeterminate, never Fail, got {other:?}"
        ),
    }
    assert!(
        matches!(result.verdict, Verdict::Indeterminate(_)),
        "the gate passes and the cq is Indeterminate, which outranks Pass, so the \
         aggregate verdict must be Indeterminate too, got {:?}",
        result.verdict
    );
}
