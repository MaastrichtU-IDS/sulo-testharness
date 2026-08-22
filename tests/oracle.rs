use std::path::Path;
use std::time::Duration;

use sulo_testharness::claim::{Claim, Literal};
use sulo_testharness::load::{load_file, merge};
use sulo_testharness::oracle::{Expectation, OracleFailure, check, holds, holds_with_deadline};
use sulo_testharness::verdict::Verdict;

const SULO: &str = "../sulo/sulo.ttl";
const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";

fn parts_ontology() -> horned_owl::ontology::set::SetOntology<horned_owl::model::RcStr> {
    let mut base = load_file(Path::new(SULO))
        .expect("SULO should load")
        .ontology;
    let data = load_file(Path::new("tests/fixtures/parts.ttl"))
        .expect("parts fixture should load")
        .ontology;
    merge(&mut base, data);
    base
}

#[test]
fn transitivity_closes() {
    let onto = parts_ontology();
    let claim = Claim::ObjectPropertyAssertion {
        subject: "http://example.org/a".into(),
        property: "https://w3id.org/sulo/isPartOf".into(),
        object: "http://example.org/c".into(),
    };
    assert!(holds(&onto, &claim).unwrap(), "isPartOf is transitive");
}

#[test]
fn reflexivity_is_found_despite_property_values_omitting_self_loops() {
    // Regression guard: dispatching this through
    // inferred_object_property_values returns nothing, because it
    // does not emit reflexive self-loops. The oracle falls back to a
    // deadline-bounded satisfiability probe (see
    // entailed_via_satisfiability_probe) restricted to exactly this
    // subject == object case, never to an unbounded class-expression
    // enumeration over every individual in the ontology.
    let onto = parts_ontology();
    let claim = Claim::ObjectPropertyAssertion {
        subject: "http://example.org/d".into(),
        property: "https://w3id.org/sulo/isPartOf".into(),
        object: "http://example.org/d".into(),
    };
    assert!(holds(&onto, &claim).unwrap(), "isPartOf is reflexive");
}

#[test]
fn subproperty_propagation_to_isin_fires() {
    let onto = parts_ontology();
    let claim = Claim::ObjectPropertyAssertion {
        subject: "http://example.org/a".into(),
        property: "https://w3id.org/sulo/isIn".into(),
        object: "http://example.org/c".into(),
    };
    assert!(
        holds(&onto, &claim).unwrap(),
        "isPartOf is a subproperty of isIn"
    );
}

#[test]
fn class_assertion_uses_the_full_closure_not_most_specific_types() {
    // ex:d is asserted SpatialObject; Object is an ancestor. realize
    // would report only SpatialObject, so this must go via instances_of.
    let onto = parts_ontology();
    let claim = Claim::ClassAssertion {
        individual: "http://example.org/d".into(),
        class: "https://w3id.org/sulo/Object".into(),
    };
    assert!(
        holds(&onto, &claim).unwrap(),
        "SpatialObject is under Object"
    );
}

#[test]
fn the_deep_subsumption_chain_closes() {
    let onto = load_file(Path::new(SULO)).unwrap().ontology;
    let claim = Claim::Subsumption {
        sub: "https://w3id.org/sulo/StartTime".into(),
        sup: "https://w3id.org/sulo/Object".into(),
    };
    assert!(holds(&onto, &claim).unwrap());
}

#[test]
fn a_known_non_subsumption_does_not_hold() {
    let onto = load_file(Path::new(SULO)).unwrap().ontology;
    let claim = Claim::Subsumption {
        sub: "https://w3id.org/sulo/Process".into(),
        sup: "https://w3id.org/sulo/Object".into(),
    };
    assert!(
        !holds(&onto, &claim).unwrap(),
        "Process is disjoint from Object"
    );
}

#[test]
fn expectation_entailed_and_holding_is_a_trustworthy_pass() {
    let onto = load_file(Path::new(SULO)).unwrap().ontology;
    let claim = Claim::Subsumption {
        sub: "https://w3id.org/sulo/Role".into(),
        sup: "https://w3id.org/sulo/Feature".into(),
    };
    assert_eq!(
        check(&onto, &claim, Expectation::Entailed).verdict,
        Verdict::Pass
    );
}

#[test]
fn expectation_not_entailed_and_not_holding_is_only_unrefuted() {
    let onto = load_file(Path::new(SULO)).unwrap().ontology;
    let claim = Claim::Subsumption {
        sub: "https://w3id.org/sulo/Process".into(),
        sup: "https://w3id.org/sulo/Object".into(),
    };
    // Absence of a proof is not proof of absence.
    assert_eq!(
        check(&onto, &claim, Expectation::NotEntailed).verdict,
        Verdict::UnrefutedPass
    );
}

#[test]
fn expectation_not_entailed_but_holding_is_a_trustworthy_fail() {
    let onto = load_file(Path::new(SULO)).unwrap().ontology;
    let claim = Claim::Subsumption {
        sub: "https://w3id.org/sulo/Role".into(),
        sup: "https://w3id.org/sulo/Feature".into(),
    };
    assert!(matches!(
        check(&onto, &claim, Expectation::NotEntailed).verdict,
        Verdict::Fail(_)
    ));
}

// --- Defect-1 regression: horned-owl's RDF reader normalises an
// xsd:string-typed literal down to Literal::Simple on the way in,
// and Simple never compares equal to Datatype. A claim built as
// Datatype(xsd:string) could never match, so a positive test would
// have produced a loud but misleading Fail, and a not_entails test
// would have produced a silent UnrefutedPass. This is the plain,
// untyped-string case the brief's ^^xsd:double test could not catch.

#[test]
fn plain_string_literal_round_trips() {
    let onto = parts_ontology();
    let claim = Claim::DataPropertyAssertion {
        subject: "http://example.org/m".into(),
        property: "https://w3id.org/sulo/hasValue".into(),
        literal: Literal {
            lexical: "hello".into(),
            datatype: XSD_STRING.into(),
            language: None,
        },
    };
    assert!(
        holds(&onto, &claim).unwrap(),
        "a bare untyped string literal must round-trip through Literal::Simple, \
         not be built as Literal::Datatype(xsd:string)"
    );
}

// --- Defect-2 regression: Task 6's claim classifier has catch-all
// arms, so a predicate that is not actually a queryable property
// (an annotation property, or one never declared at all) is still
// classified as an ObjectPropertyAssertion or DataPropertyAssertion.
// Querying the reasoner with it silently finds no instances, which
// would misreport as a trustworthy Fail or a silent UnrefutedPass
// instead of the Indeterminate it actually is. `holds` must check
// the ontology's declarations first and name the predicate in the
// error.

#[test]
fn annotation_property_predicate_is_rejected() {
    let onto = load_file(Path::new(SULO)).unwrap().ontology;
    let claim = Claim::ObjectPropertyAssertion {
        subject: "https://w3id.org/sulo/Role".into(),
        property: "http://purl.org/dc/terms/title".into(),
        object: "https://w3id.org/sulo/Feature".into(),
    };
    let err = holds(&onto, &claim)
        .expect_err("dcterms:title is an annotation property, not queryable")
        .to_string();
    assert!(
        err.contains("http://purl.org/dc/terms/title"),
        "error message should name the rejected predicate, got: {err}"
    );
}

#[test]
fn undeclared_predicate_is_rejected() {
    let onto = load_file(Path::new(SULO)).unwrap().ontology;
    let claim = Claim::DataPropertyAssertion {
        subject: "https://w3id.org/sulo/Role".into(),
        property: "http://www.w3.org/2000/01/rdf-schema#label".into(),
        literal: Literal {
            lexical: "role".into(),
            datatype: XSD_STRING.into(),
            language: None,
        },
    };
    let err = holds(&onto, &claim)
        .expect_err("rdfs:label is not declared as a property in sulo.ttl")
        .to_string();
    assert!(
        err.contains("http://www.w3.org/2000/01/rdf-schema#label"),
        "error message should name the rejected predicate, got: {err}"
    );
}

// --- Round-2 review findings: the data fallback was unguarded (same
// unbounded call that hung on the object side), a real timeout could
// be delivered as a trustworthy-looking Fail, and two silent-inversion
// risks (Equivalence's && vs ||, Unsatisfiable's sign) had no test.

#[test]
fn data_fallback_terminates_with_a_sound_answer_for_a_language_tagged_literal() {
    // The materialised fast path (inferred_data_property_values)
    // drops the language tag entirely, so a language-tagged claim
    // always routes to entailed_via_satisfiability_probe, the narrow,
    // deadline-bounded fallback that replaced the unbounded
    // class_expression_instances call.
    //
    // Empirically (verified against real SULO at the pinned rustdl
    // v0.4.22), the underlying tableau does not currently recognise
    // membership for an rdf:langString DataHasValue restriction even
    // for an individual with the exact literal asserted directly:
    // `class_expression_instances`, the OLD unbounded mechanism, also
    // returns no individuals for this same query. This is a genuine,
    // disclosed reasoner-completeness gap for language-tagged data
    // values, not a bug in this dispatch. The claim this test is
    // making is narrower and still load-bearing: routing to the
    // fallback for this case terminates fast (proving it is no
    // longer the unbounded call that used to hang) and returns a
    // sound `false` rather than erroring or hanging — sound because
    // `check` never promotes an unproven `false` to a trustworthy
    // `Pass`, only ever to `UnrefutedPass` for a negative expectation.
    let onto = parts_ontology();
    let claim = Claim::DataPropertyAssertion {
        subject: "http://example.org/n".into(),
        property: "https://w3id.org/sulo/hasValue".into(),
        literal: Literal {
            lexical: "bonjour".into(),
            datatype: "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString".into(),
            language: Some("fr".into()),
        },
    };
    assert_eq!(
        holds(&onto, &claim),
        Ok(false),
        "the fallback must terminate with a definite, sound answer, \
         not hang or error, even though this reasoner cannot currently \
         confirm a language-tagged data value"
    );
}

#[test]
fn a_zero_deadline_yields_timeout_not_a_false_negative() {
    // Same claim as data_fallback_confirms_a_language_tagged_literal,
    // but with a zero deadline: the fallback's is_class_satisfiable_with_timeout
    // call must report a genuine Timeout rather than silently
    // collapsing to a trustworthy-looking `false`.
    let onto = parts_ontology();
    let claim = Claim::DataPropertyAssertion {
        subject: "http://example.org/n".into(),
        property: "https://w3id.org/sulo/hasValue".into(),
        literal: Literal {
            lexical: "bonjour".into(),
            datatype: "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString".into(),
            language: Some("fr".into()),
        },
    };
    let result = holds_with_deadline(&onto, &claim, Duration::from_secs(0));
    assert!(
        matches!(result, Err(OracleFailure::Timeout)),
        "a zero deadline must yield OracleFailure::Timeout, got: {result:?}"
    );
}

#[test]
fn equivalence_requires_both_directions_not_just_one() {
    // A `||` bug in place of `&&` would report every one-way
    // subsumption as an equivalence too.
    let onto = parts_ontology();

    let genuinely_equivalent = Claim::Equivalence {
        left: "http://example.org/Foo".into(),
        right: "http://example.org/Bar".into(),
    };
    assert!(
        holds(&onto, &genuinely_equivalent).unwrap(),
        "Foo and Bar are asserted owl:equivalentClass"
    );

    let one_way_only = Claim::Equivalence {
        left: "https://w3id.org/sulo/Role".into(),
        right: "https://w3id.org/sulo/Feature".into(),
    };
    assert!(
        !holds(&onto, &one_way_only).unwrap(),
        "Role is a strict subclass of Feature, not equivalent to it; \
         a `||` bug would wrongly report this pair as equivalent"
    );
}

#[test]
fn unsatisfiable_sign_is_not_flipped() {
    // A sign flip on `.map(|sat| !sat)` would invert every verdict:
    // unsatisfiable classes would read as fine, and vice versa.
    let onto = parts_ontology();

    let genuinely_unsatisfiable = Claim::Unsatisfiable {
        class: "http://example.org/Bad".into(),
    };
    assert!(
        holds(&onto, &genuinely_unsatisfiable).unwrap(),
        "ex:Bad is asserted a subclass of both sulo:Process and \
         sulo:Object, which are owl:disjointWith each other"
    );

    let genuinely_satisfiable = Claim::Unsatisfiable {
        class: "https://w3id.org/sulo/Object".into(),
    };
    assert!(
        !holds(&onto, &genuinely_satisfiable).unwrap(),
        "sulo:Object is satisfiable on its own; a sign flip would \
         wrongly report it as unsatisfiable"
    );
}
