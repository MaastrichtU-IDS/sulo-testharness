use std::collections::BTreeMap;
use sulo_testharness::prefixes::{base_mapping, expand, with_overrides};

#[test]
fn sulo_and_the_standard_prefixes_are_always_bound() {
    let pm = base_mapping();
    assert_eq!(expand(&pm, "sulo:Process").unwrap(), "https://w3id.org/sulo/Process");
    assert_eq!(
        expand(&pm, "owl:Thing").unwrap(),
        "http://www.w3.org/2002/07/owl#Thing"
    );
    assert_eq!(
        expand(&pm, "rdfs:subClassOf").unwrap(),
        "http://www.w3.org/2000/01/rdf-schema#subClassOf"
    );
}

#[test]
fn case_overrides_win() {
    let mut over = BTreeMap::new();
    over.insert("ex".to_string(), "http://example.org/".to_string());
    let pm = with_overrides(&base_mapping(), &over);
    assert_eq!(expand(&pm, "ex:alice").unwrap(), "http://example.org/alice");
}

#[test]
fn a_full_iri_passes_through() {
    let pm = base_mapping();
    assert_eq!(
        expand(&pm, "<http://example.org/x>").unwrap(),
        "http://example.org/x"
    );
}

#[test]
fn an_unbound_prefix_is_an_error_not_a_guess() {
    let pm = base_mapping();
    assert!(expand(&pm, "nope:thing").is_err());
}
