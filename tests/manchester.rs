use std::path::Path;

use sulo_testharness::claim::parse_ce;
use sulo_testharness::load::{load_file, merge};
use sulo_testharness::oracle::{
    Expectation, check_instance_expr, check_satisfiable_expr, check_subsumption_expr,
};
use sulo_testharness::prefixes::base_mapping;
use sulo_testharness::verdict::Verdict;

const SULO: &str = "../sulo/sulo.ttl";

/// SULO plus a small PRO-pattern (Process/Role/Object) fixture: an
/// encounter (Process) with a participant that is a Role, whose
/// isFeatureOf points at an Object. Used to exercise
/// `check_instance_expr` and `check_satisfiable_expr` against a
/// pattern that is actually satisfiable under SULO's real axioms
/// (Role ⊑ Feature ⊑ Object, so the participant's inferred Object
/// membership via `hasParticipant`'s range does not clash with its
/// asserted Role membership).
fn pro_pattern_ontology() -> horned_owl::ontology::set::SetOntology<horned_owl::model::RcStr> {
    let mut base = load_file(Path::new(SULO))
        .expect("SULO should load")
        .ontology;
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
    let onto = load_file(Path::new(SULO)).unwrap().ontology;
    let out = check_subsumption_expr(
        &onto,
        "sulo:Feature",
        "sulo:Capability or sulo:InformationObject or sulo:Quality or sulo:Role",
        Expectation::Entailed,
        &base_mapping(),
    );
    assert_eq!(
        out.verdict,
        Verdict::Pass,
        "the Feature disjoint union covers"
    );
}

#[test]
fn object_non_covering_is_not_entailed() {
    let onto = load_file(Path::new(SULO)).unwrap().ontology;
    let out = check_subsumption_expr(
        &onto,
        "sulo:Object",
        "sulo:SpatialObject or sulo:Feature",
        Expectation::NotEntailed,
        &base_mapping(),
    );
    // Object deliberately has no covering axiom.
    assert_eq!(out.verdict, Verdict::UnrefutedPass);
}

#[test]
fn a_tautology_is_entailed_even_without_the_ontology() {
    // Guard against the schema example regression: C and D <= C holds
    // in any ontology, so such a case proves nothing. This test
    // documents the trap rather than endorsing it.
    let onto = load_file(Path::new(SULO)).unwrap().ontology;
    let out = check_subsumption_expr(
        &onto,
        "sulo:Process and sulo:hasParticipant some sulo:Role",
        "sulo:Process",
        Expectation::Entailed,
        &base_mapping(),
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
    );
    assert_eq!(out.verdict, Verdict::UnrefutedPass);
}

#[test]
fn satisfiable_expr_holds_for_the_pro_pattern() {
    // Same expression as above, asked as a pure satisfiability
    // question rather than about a specific individual: guards the
    // pattern itself from silently becoming unsatisfiable if SULO's
    // axioms about Role/Object/Feature ever change.
    let onto = pro_pattern_ontology();
    let out = check_satisfiable_expr(
        &onto,
        "sulo:Process and sulo:hasParticipant some (sulo:Role and sulo:isFeatureOf some sulo:Object)",
        Expectation::Entailed,
        &base_mapping(),
    );
    assert_eq!(
        out.verdict,
        Verdict::Pass,
        "the PRO pattern must remain satisfiable, got {out:?}"
    );
}

#[test]
fn satisfiable_expr_fails_for_a_genuinely_unsatisfiable_expression() {
    // Process and Object are declared disjoint in SULO, so no
    // individual can be both.
    let onto = load_file(Path::new(SULO)).unwrap().ontology;
    let out = check_satisfiable_expr(
        &onto,
        "sulo:Process and sulo:Object",
        Expectation::Entailed,
        &base_mapping(),
    );
    assert!(
        matches!(out.verdict, Verdict::Fail(_)),
        "expected Fail, got {out:?}"
    );
}
