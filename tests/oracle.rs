use std::path::Path;

use sulo_testharness::claim::{Claim, Literal};
use sulo_testharness::load::{load_file, merge};
use sulo_testharness::oracle::{Expectation, check, holds};
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
    // does not emit reflexive self-loops. The oracle must use a
    // class-expression query instead.
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
    let err =
        holds(&onto, &claim).expect_err("dcterms:title is an annotation property, not queryable");
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
    let err =
        holds(&onto, &claim).expect_err("rdfs:label is not declared as a property in sulo.ttl");
    assert!(
        err.contains("http://www.w3.org/2000/01/rdf-schema#label"),
        "error message should name the rejected predicate, got: {err}"
    );
}
