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

#[test]
fn ordered_true_with_exact_false_is_a_configuration_error() {
    // Spec 7.3 does not disambiguate this combination: "expected is a
    // contiguous prefix of actual" and "expected is a non-contiguous
    // ordered subsequence, extras allowed anywhere" are equally licensed.
    // Refuse rather than silently pick a reading.
    let e = vec![row(&[("p", Some(iri("http://example.org/a")))])];
    let a = vec![row(&[("p", Some(iri("http://example.org/a")))])];
    let err = compare(&e, &a, false, true).unwrap_err();
    assert!(
        err.contains("ordered") && err.contains("exact"),
        "the message must name both flags: {err}"
    );
}

#[test]
fn ordered_positional_mismatch_names_both_rows() {
    let e = vec![row(&[("p", Some(iri("http://example.org/expected_here")))])];
    let a = vec![row(&[("p", Some(iri("http://example.org/actual_here")))])];
    let err = compare(&e, &a, true, true).unwrap_err();
    assert!(
        err.contains("expected_here") && err.contains("actual_here"),
        "the message must name what was expected and what was found: {err}"
    );
}

#[test]
fn ordered_missing_position_names_the_missing_row() {
    let e = vec![
        row(&[("p", Some(iri("http://example.org/a")))]),
        row(&[("p", Some(iri("http://example.org/missing_position")))]),
    ];
    let a = vec![row(&[("p", Some(iri("http://example.org/a")))])];
    let err = compare(&e, &a, true, true).unwrap_err();
    assert!(
        err.contains("missing_position"),
        "the message must name the missing row: {err}"
    );
}

#[test]
fn ordered_exact_rejects_a_trailing_extra_row() {
    // Drives the ordered+exact leftover-length branch to completion: every
    // position up to expected.len() matches, but actual is longer.
    let e = vec![row(&[("p", Some(iri("http://example.org/a")))])];
    let a = vec![
        row(&[("p", Some(iri("http://example.org/a")))]),
        row(&[("p", Some(iri("http://example.org/trailing_extra")))]),
    ];
    let err = compare(&e, &a, true, true).unwrap_err();
    assert!(
        err.contains("trailing_extra"),
        "the message must name the extra row: {err}"
    );
}

#[test]
fn unordered_exact_extra_row_is_named() {
    let e = vec![row(&[("p", Some(iri("http://example.org/a")))])];
    let a = vec![
        row(&[("p", Some(iri("http://example.org/a")))]),
        row(&[("p", Some(iri("http://example.org/unordered_extra")))]),
    ];
    let err = compare(&e, &a, true, false).unwrap_err();
    assert!(
        err.contains("unordered_extra"),
        "the message must name an extra row: {err}"
    );
}

#[test]
fn an_unrecognised_escape_in_a_literal_is_an_error() {
    // \x is not one of the recognised escapes; silently passing it
    // through as literal backslash-x would surprise a suite author who
    // meant something else by it.
    let err = parse_expected(Some(r#""ba\x""#), &base_mapping()).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("\\x"),
        "the message must name the offending escape: {msg}"
    );
}
