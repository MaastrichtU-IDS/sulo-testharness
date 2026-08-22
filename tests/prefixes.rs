use std::collections::BTreeMap;
use sulo_testharness::prefixes::{base_mapping, expand, with_overrides, PrefixError};

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
fn overrides_replace_existing_base_prefixes() {
    let mut over = BTreeMap::new();
    over.insert("sulo".to_string(), "http://example.org/custom-sulo/".to_string());
    let pm = with_overrides(&base_mapping(), &over);
    assert_eq!(
        expand(&pm, "sulo:Process").unwrap(),
        "http://example.org/custom-sulo/Process"
    );
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
    let err = expand(&pm, "nope:thing").expect_err("unbound should error");
    match err {
        PrefixError::Unbound { prefix } => assert_eq!(prefix, "nope"),
        _ => panic!("expected Unbound, got {err}"),
    }
}

#[test]
fn malformed_iri_without_closing_bracket_is_malformed_not_unbound() {
    let pm = base_mapping();
    let err = expand(&pm, "<http://example.org/x").expect_err("malformed should error");
    match err {
        PrefixError::Malformed(s) => assert_eq!(s, "<http://example.org/x"),
        _ => panic!("expected Malformed, got {err}"),
    }
}

#[test]
fn bare_word_without_colon_is_malformed() {
    let pm = base_mapping();
    let err = expand(&pm, "bareword").expect_err("bareword should error");
    match err {
        PrefixError::Malformed(s) => assert_eq!(s, "bareword"),
        _ => panic!("expected Malformed, got {err}"),
    }
}
