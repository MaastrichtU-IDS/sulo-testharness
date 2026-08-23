use std::path::Path;

use sulo_testharness::claim::parse_ce;
use sulo_testharness::load::{load_file, merge};
use sulo_testharness::oracle::{
    Expectation, NO_PROOF_MARKER, REASONER_DEADLINE, check_instance_expr, check_satisfiable_expr,
    check_subsumption_expr,
};
use sulo_testharness::prefixes::base_mapping;
use sulo_testharness::verdict::{IndeterminateReason, Verdict};

const SULO: &str = "../sulo/sulo.ttl";

/// Real SULO, loaded with an explicit prerequisite check.
///
/// Every test here reads `../sulo/sulo.ttl` by relative path, so a
/// checkout without the sulo repo as a sibling directory would
/// otherwise panic on `.unwrap()` with a bare `Io` error that reads
/// like a harness bug. `mutants/regenerate.sh` already guards the same
/// prerequisite with an explicit message; this is that message.
fn sulo_ontology() -> horned_owl::ontology::set::SetOntology<horned_owl::model::RcStr> {
    assert!(
        Path::new(SULO).is_file(),
        "{SULO} not found. These tests read real SULO by relative path, so the \
         sulo repo must be checked out as a sibling of sulo-testharness \
         (the same prerequisite mutants/regenerate.sh checks for)."
    );
    load_file(Path::new(SULO))
        .expect("real SULO should load")
        .ontology
}

/// SULO plus a small PRO-pattern (Process/Role/Object) fixture: an
/// encounter (Process) with a participant that is a Role, whose
/// isFeatureOf points at an Object. Used to exercise
/// `check_instance_expr` and `check_satisfiable_expr` against a
/// pattern that is actually satisfiable under SULO's real axioms
/// (Role ⊑ Feature ⊑ Object, so the participant's inferred Object
/// membership via `hasParticipant`'s range does not clash with its
/// asserted Role membership).
fn pro_pattern_ontology() -> horned_owl::ontology::set::SetOntology<horned_owl::model::RcStr> {
    let mut base = sulo_ontology();
    let data = load_file(Path::new("tests/fixtures/pro-pattern.ttl"))
        .expect("pro-pattern fixture should load")
        .ontology;
    merge(&mut base, data);
    base
}

#[test]
fn curies_resolve_via_the_prefix_map() {
    // No rewriting to full <IRI> needed: parse_class_expression takes
    // the PrefixMapping directly.
    let ce = parse_ce("sulo:Capability or sulo:Role", &base_mapping());
    assert!(ce.is_ok(), "expected a parse, got {ce:?}");
}

#[test]
fn feature_covering_is_entailed() {
    let onto = sulo_ontology();
    let out = check_subsumption_expr(
        &onto,
        "sulo:Feature",
        "sulo:Capability or sulo:InformationObject or sulo:Quality or sulo:Role",
        Expectation::Entailed,
        &base_mapping(),
        REASONER_DEADLINE,
    );
    assert_eq!(
        out.verdict,
        Verdict::Pass,
        "the Feature disjoint union covers"
    );
}

#[test]
fn object_non_covering_is_not_entailed() {
    let onto = sulo_ontology();
    let out = check_subsumption_expr(
        &onto,
        "sulo:Object",
        "sulo:SpatialObject or sulo:Feature",
        Expectation::NotEntailed,
        &base_mapping(),
        REASONER_DEADLINE,
    );
    // Object deliberately has no covering axiom.
    assert_eq!(out.verdict, Verdict::UnrefutedPass);
}

#[test]
fn a_tautology_is_entailed_even_without_the_ontology() {
    // Guard against the schema example regression: C and D <= C holds
    // in any ontology, so such a case proves nothing. This test
    // documents the trap rather than endorsing it.
    let onto = sulo_ontology();
    let out = check_subsumption_expr(
        &onto,
        "sulo:Process and sulo:hasParticipant some sulo:Role",
        "sulo:Process",
        Expectation::Entailed,
        &base_mapping(),
        REASONER_DEADLINE,
    );
    assert_eq!(out.verdict, Verdict::Pass);
}

#[test]
fn instance_of_expr_is_entailed_for_the_pro_pattern() {
    // ex:encounter is a Process whose participant (ex:doctorRole) is a
    // Role that isFeatureOf an Object (ex:alice). This is what a
    // covering claim over a real PRO-shaped competency question looks
    // like: no ground triple states "encounter is in this class", it
    // is only entailed through the composite expression.
    let onto = pro_pattern_ontology();
    let out = check_instance_expr(
        &onto,
        "http://example.org/encounter",
        "sulo:Process and sulo:hasParticipant some (sulo:Role and sulo:isFeatureOf some sulo:Object)",
        Expectation::Entailed,
        &base_mapping(),
        REASONER_DEADLINE,
    );
    assert_eq!(
        out.verdict,
        Verdict::Pass,
        "encounter should provably be a member of the PRO pattern, got {out:?}"
    );
}

#[test]
fn instance_of_expr_is_not_entailed_for_an_unrelated_individual() {
    let onto = pro_pattern_ontology();
    let out = check_instance_expr(
        &onto,
        "http://example.org/alice",
        "sulo:Process and sulo:hasParticipant some (sulo:Role and sulo:isFeatureOf some sulo:Object)",
        Expectation::NotEntailed,
        &base_mapping(),
        REASONER_DEADLINE,
    );
    assert_eq!(out.verdict, Verdict::UnrefutedPass);
}

#[test]
fn satisfiable_expr_holds_for_the_pro_pattern() {
    // Same expression as above, asked as a pure satisfiability
    // question rather than about a specific individual: guards the
    // pattern itself from silently becoming unsatisfiable if SULO's
    // axioms about Role/Object/Feature ever change.
    //
    // UnrefutedPass, not Pass, and that is the whole point. The probe
    // behind this check answers "is it UNsatisfiable?", and UNSAT is
    // its only trustworthy direction: SAT is what a missed clash also
    // produces. So "expect satisfiable, and it is" rests on an absence
    // of proof and must be reported as the harness's non-failing,
    // separately-counted verdict, exactly like every other negative
    // expectation the reasoner failed to refute. An earlier revision
    // reported Pass here, a verified pass over an absence of proof;
    // see `check_satisfiable_expr`'s doc.
    let onto = pro_pattern_ontology();
    let out = check_satisfiable_expr(
        &onto,
        "sulo:Process and sulo:hasParticipant some (sulo:Role and sulo:isFeatureOf some sulo:Object)",
        Expectation::Entailed,
        &base_mapping(),
        REASONER_DEADLINE,
    );
    assert_eq!(
        out.verdict,
        Verdict::UnrefutedPass,
        "the PRO pattern must remain satisfiable, and satisfiability is the \
         unprovable direction of this probe, got {out:?}"
    );
}

#[test]
fn satisfiable_expr_fails_for_a_genuinely_unsatisfiable_expression() {
    // Process and Object are declared disjoint in SULO, so no
    // individual can be both.
    let onto = sulo_ontology();
    let out = check_satisfiable_expr(
        &onto,
        "sulo:Process and sulo:Object",
        Expectation::Entailed,
        &base_mapping(),
        REASONER_DEADLINE,
    );
    let Verdict::Fail(msg) = &out.verdict else {
        panic!("expected Fail, got {out:?}");
    };
    // The DISCRIMINATING half: this Fail must be the trustworthy
    // "a proof was found that contradicts the expectation" shape, not
    // the untrusted "no proof was found" shape. The earlier revision
    // produced the latter, which `suite::downgrade_for_loss` then
    // demoted to Indeterminate(AxiomLoss), so a genuine provable
    // unsatisfiability regression could vanish into an Indeterminate.
    assert!(
        msg.contains("expected NOT to hold, but it does"),
        "an unsatisfiable expression is PROVABLY so; the failure must say a \
         proof was found, got: {msg}"
    );
    assert!(
        !msg.contains(NO_PROOF_MARKER),
        "this Fail must not carry the absence-of-proof marker, or the axiom-loss \
         downgrade would demote a trustworthy regression to Indeterminate, got: {msg}"
    );
}

// ---------------------------------------------------------------
// Undeclared terms in a Manchester expression. `parse_ce` never
// consults the ontology and rustdl's conversion registers any IRI it
// meets, so a typo silently becomes a fresh unconstrained entity:
// trivially satisfiable, trivially not subsumed, hence a green
// verdict for a case that tests nothing. Both of these must be
// Indeterminate(OracleError) naming the offending term.
// ---------------------------------------------------------------

fn oracle_error(out: &sulo_testharness::verdict::CheckOutcome) -> &str {
    match &out.verdict {
        Verdict::Indeterminate(IndeterminateReason::OracleError(msg)) => msg,
        other => panic!("expected Indeterminate(OracleError), got {other:?}"),
    }
}

#[test]
fn a_typod_class_in_a_manchester_expression_is_indeterminate_not_green() {
    let onto = sulo_ontology();

    // Positive direction: without the declaration check this is a
    // trivially satisfiable fresh class, reported as a pass.
    let sat = check_satisfiable_expr(
        &onto,
        "sulo:Featuer and sulo:Role",
        Expectation::Entailed,
        &base_mapping(),
        REASONER_DEADLINE,
    );
    assert!(
        oracle_error(&sat).contains("https://w3id.org/sulo/Featuer"),
        "the message must name the undeclared class, got: {}",
        oracle_error(&sat)
    );

    // Negative direction: without the check, a typo in a
    // `not_entails_manchester` case is trivially not entailed and
    // yields a silent UnrefutedPass, the worse of the two failures.
    let sub = check_subsumption_expr(
        &onto,
        "sulo:Role",
        "sulo:Featuer",
        Expectation::NotEntailed,
        &base_mapping(),
        REASONER_DEADLINE,
    );
    assert!(
        oracle_error(&sub).contains("https://w3id.org/sulo/Featuer"),
        "the message must name the undeclared class, got: {}",
        oracle_error(&sub)
    );
}

#[test]
fn a_typod_property_in_a_manchester_expression_is_indeterminate_not_green() {
    let onto = pro_pattern_ontology();
    let out = check_instance_expr(
        &onto,
        "http://example.org/encounter",
        "sulo:Process and sulo:hasParticipnt some sulo:Role",
        Expectation::Entailed,
        &base_mapping(),
        REASONER_DEADLINE,
    );
    assert!(
        oracle_error(&out).contains("https://w3id.org/sulo/hasParticipnt"),
        "the message must name the undeclared object property, got: {}",
        oracle_error(&out)
    );
}

#[test]
fn a_typod_individual_in_an_instance_check_is_indeterminate_not_green() {
    let onto = pro_pattern_ontology();
    let out = check_instance_expr(
        &onto,
        "http://example.org/encountr",
        "sulo:Process",
        Expectation::Entailed,
        &base_mapping(),
        REASONER_DEADLINE,
    );
    assert!(
        oracle_error(&out).contains("http://example.org/encountr"),
        "the message must name the unknown individual, got: {}",
        oracle_error(&out)
    );
}
