use std::collections::BTreeMap;

use sulo_testharness::claim::{Claim, parse_fragment};
use sulo_testharness::prefixes::{base_mapping, with_overrides};

fn pm() -> curie::PrefixMapping {
    let mut over = BTreeMap::new();
    over.insert("ex".to_string(), "http://example.org/".to_string());
    with_overrides(&base_mapping(), &over)
}

#[test]
fn classifies_a_subsumption() {
    let claims = parse_fragment("sulo:Role rdfs:subClassOf sulo:Feature .", &pm()).unwrap();
    assert_eq!(claims.len(), 1);
    match &claims[0] {
        Claim::Subsumption { sub, sup } => {
            assert_eq!(sub, "https://w3id.org/sulo/Role");
            assert_eq!(sup, "https://w3id.org/sulo/Feature");
        }
        other => panic!("expected Subsumption, got {other:?}"),
    }
}

#[test]
fn subclassof_nothing_becomes_unsatisfiable() {
    let claims = parse_fragment("sulo:Role rdfs:subClassOf owl:Nothing .", &pm()).unwrap();
    assert!(matches!(&claims[0], Claim::Unsatisfiable { .. }));
}

#[test]
fn classifies_a_class_assertion() {
    let claims = parse_fragment("ex:alice a sulo:SpatialObject .", &pm()).unwrap();
    match &claims[0] {
        Claim::ClassAssertion { individual, class } => {
            assert_eq!(individual, "http://example.org/alice");
            assert_eq!(class, "https://w3id.org/sulo/SpatialObject");
        }
        other => panic!("expected ClassAssertion, got {other:?}"),
    }
}

#[test]
fn classifies_an_object_property_assertion() {
    let claims = parse_fragment("ex:encounter sulo:hasParticipant ex:alice .", &pm()).unwrap();
    assert!(matches!(&claims[0], Claim::ObjectPropertyAssertion { .. }));
}

#[test]
fn classifies_a_typed_data_property_assertion() {
    let claims = parse_fragment(r#"ex:m sulo:hasValue "37.8"^^xsd:double ."#, &pm()).unwrap();
    match &claims[0] {
        Claim::DataPropertyAssertion { literal, .. } => {
            assert_eq!(literal.lexical, "37.8");
            assert_eq!(literal.datatype, "http://www.w3.org/2001/XMLSchema#double");
        }
        other => panic!("expected DataPropertyAssertion, got {other:?}"),
    }
}

#[test]
fn classifies_an_untyped_data_property_assertion() {
    // A bare Turtle literal with no `^^` suffix. What datatype IRI does
    // oxrdf actually report here? Recorded in task-6-report.md.
    let claims = parse_fragment(r#"ex:m sulo:hasNote "plain text" ."#, &pm()).unwrap();
    match &claims[0] {
        Claim::DataPropertyAssertion { literal, .. } => {
            assert_eq!(literal.lexical, "plain text");
            assert_eq!(literal.datatype, "http://www.w3.org/2001/XMLSchema#string");
            assert_eq!(literal.language, None);
        }
        other => panic!("expected DataPropertyAssertion, got {other:?}"),
    }
}

#[test]
fn multiple_statements_yield_multiple_claims() {
    let f = "ex:encounter sulo:hasParticipant ex:alice, ex:drsmith .";
    assert_eq!(parse_fragment(f, &pm()).unwrap().len(), 2);
}

#[test]
fn a_blank_node_subject_is_rejected() {
    // Blank nodes cannot be addressed by a reasoner query and never
    // compare equal across runs.
    assert!(parse_fragment("_:b sulo:isPartOf ex:a .", &pm()).is_err());
}

#[test]
fn an_unbound_prefix_is_an_error() {
    assert!(parse_fragment("nope:x sulo:isPartOf ex:a .", &pm()).is_err());
}
