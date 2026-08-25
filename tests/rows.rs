use std::collections::BTreeMap;

use oxrdf::{Literal, NamedNode, Term};
use sulo_testharness::prefixes::base_mapping;
use sulo_testharness::rows::{Expected, compare, parse_expected};

fn iri(s: &str) -> Term {
    Term::NamedNode(NamedNode::new(s).unwrap())
}

fn row(pairs: &[(&str, Option<Term>)]) -> BTreeMap<String, Option<Term>> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), v.clone()))
        .collect()
}

#[test]
fn a_curie_expands_to_an_iri_term() {
    let e = parse_expected(Some("sulo:Process"), &base_mapping()).unwrap();
    assert_eq!(e, Expected::Bound(iri("https://w3id.org/sulo/Process")));
}

#[test]
fn an_angle_bracket_iri_passes_through() {
    let e = parse_expected(Some("<http://example.org/x>"), &base_mapping()).unwrap();
    assert_eq!(e, Expected::Bound(iri("http://example.org/x")));
}

#[test]
fn a_typed_literal_keeps_its_datatype() {
    let e = parse_expected(Some(r#""37.8"^^xsd:double"#), &base_mapping()).unwrap();
    let want = Term::Literal(Literal::new_typed_literal(
        "37.8",
        NamedNode::new("http://www.w3.org/2001/XMLSchema#double").unwrap(),
    ));
    assert_eq!(e, Expected::Bound(want));
}

#[test]
fn a_bare_literal_is_an_xsd_string_not_a_wildcard() {
    // Spec 7.3: literal equality is RDF TERM equality, so a bare
    // literal is xsd:string and does NOT equal "37.8"^^xsd:double.
    let bare = parse_expected(Some(r#""37.8""#), &base_mapping()).unwrap();
    let typed = parse_expected(Some(r#""37.8"^^xsd:double"#), &base_mapping()).unwrap();
    assert_ne!(
        bare, typed,
        "value-space equality would hide serialisation regressions"
    );
}

#[test]
fn a_language_literal_keeps_its_tag() {
    let e = parse_expected(Some(r#""bonjour"@fr"#), &base_mapping()).unwrap();
    let want = Term::Literal(Literal::new_language_tagged_literal("bonjour", "fr").unwrap());
    assert_eq!(e, Expected::Bound(want));
}

#[test]
fn null_means_expected_unbound() {
    assert_eq!(
        parse_expected(None, &base_mapping()).unwrap(),
        Expected::Unbound
    );
}

#[test]
fn a_blank_node_is_a_configuration_error() {
    // Blank nodes never compare equal across runs.
    assert!(parse_expected(Some("_:b0"), &base_mapping()).is_err());
}

#[test]
fn an_unbound_prefix_is_an_error() {
    assert!(parse_expected(Some("nope:thing"), &base_mapping()).is_err());
}

#[test]
fn exact_compare_rejects_an_extra_actual_row() {
    let e = vec![row(&[("p", Some(iri("http://example.org/a")))])];
    let a = vec![
        row(&[("p", Some(iri("http://example.org/a")))]),
        row(&[("p", Some(iri("http://example.org/b")))]),
    ];
    assert!(
        compare(&e, &a, true, false).is_err(),
        "exact must reject extras"
    );
    assert!(
        compare(&e, &a, false, false).is_ok(),
        "subset must allow extras"
    );
}

#[test]
fn subset_still_rejects_a_missing_expected_row() {
    let e = vec![
        row(&[("p", Some(iri("http://example.org/a")))]),
        row(&[("p", Some(iri("http://example.org/z")))]),
    ];
    let a = vec![row(&[("p", Some(iri("http://example.org/a")))])];
    assert!(
        compare(&e, &a, false, false).is_err(),
        "subset is not 'anything goes'"
    );
}

#[test]
fn unordered_compare_ignores_position() {
    let e = vec![
        row(&[("p", Some(iri("http://example.org/a")))]),
        row(&[("p", Some(iri("http://example.org/b")))]),
    ];
    let a = vec![
        row(&[("p", Some(iri("http://example.org/b")))]),
        row(&[("p", Some(iri("http://example.org/a")))]),
    ];
    assert!(compare(&e, &a, true, false).is_ok());
    assert!(
        compare(&e, &a, true, true).is_err(),
        "ordered must respect position"
    );
}

#[test]
fn duplicate_rows_are_significant() {
    // Multiset, not set: a query returning a row twice is a different
    // answer from one returning it once.
    let e = vec![row(&[("p", Some(iri("http://example.org/a")))])];
    let a = vec![
        row(&[("p", Some(iri("http://example.org/a")))]),
        row(&[("p", Some(iri("http://example.org/a")))]),
    ];
    assert!(
        compare(&e, &a, true, false).is_err(),
        "duplicates must not collapse"
    );
}

#[test]
fn an_unbound_actual_matches_only_an_expected_unbound() {
    let e = vec![row(&[("p", None)])];
    let a_unbound = vec![row(&[("p", None)])];
    let a_bound = vec![row(&[("p", Some(iri("http://example.org/a")))])];
    assert!(compare(&e, &a_unbound, true, false).is_ok());
    assert!(compare(&e, &a_bound, true, false).is_err());
}

#[test]
fn the_error_names_what_was_missing() {
    let e = vec![row(&[("p", Some(iri("http://example.org/zzz")))])];
    let a = vec![row(&[("p", Some(iri("http://example.org/a")))])];
    let err = compare(&e, &a, true, false).unwrap_err();
    assert!(
        err.contains("zzz"),
        "the message must name the missing row: {err}"
    );
}
