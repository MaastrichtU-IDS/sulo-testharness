use std::path::Path;

use horned_owl::model::{Build, ClassExpression, Component, DeclareClass, RcStr, SubClassOf};
use horned_owl::ontology::set::SetOntology;
use sulo_testharness::load::{load_file, merge};

/// True if `onto` contains a class declaration for `iri`.
fn declares_class(onto: &SetOntology<RcStr>, iri: &str) -> bool {
    let b: Build<RcStr> = Build::new_rc();
    let expected = Component::DeclareClass(DeclareClass(b.class(iri)));
    onto.iter().any(|ac| ac.component == expected)
}

/// True if `onto` contains `sub_iri rdfs:subClassOf sup_iri`.
fn has_subclass_of(onto: &SetOntology<RcStr>, sub_iri: &str, sup_iri: &str) -> bool {
    let b: Build<RcStr> = Build::new_rc();
    let expected = Component::SubClassOf(SubClassOf {
        sup: ClassExpression::Class(b.class(sup_iri)),
        sub: ClassExpression::Class(b.class(sub_iri)),
    });
    onto.iter().any(|ac| ac.component == expected)
}

#[test]
fn loads_turtle_and_reports_alldisjointclasses_as_loss() {
    let loaded =
        load_file(Path::new("tests/fixtures/all-disjoint.ttl")).expect("fixture should parse");

    // horned-owl has no AllDisjointClasses handling: the triples land in
    // IncompleteParse. The harness must surface that, not swallow it.
    assert!(
        !loaded.loss.is_empty(),
        "AllDisjointClasses must be reported as loss, got none"
    );
    assert!(
        loaded.loss.iter().any(|d| d.contains("parse")),
        "loss should name the parse channel, got {:?}",
        loaded.loss
    );

    // The three class declarations precede the AllDisjointClasses triples
    // and are ordinary axioms the reader does handle. If they are absent,
    // no real parsing happened and the `loss` assertions above would be
    // vacuously true for a stub that never touched the file.
    for iri in [
        "http://example.org/A",
        "http://example.org/B",
        "http://example.org/C",
    ] {
        assert!(
            declares_class(&loaded.ontology, iri),
            "expected {iri} to be declared a class, got {:#?}",
            loaded.ontology
        );
    }
}

#[test]
fn a_clean_ontology_reports_no_loss() {
    let loaded = load_file(Path::new("tests/fixtures/clean.ttl")).expect("fixture should parse");
    assert!(loaded.loss.is_empty(), "unexpected loss: {:?}", loaded.loss);

    // Confirm the fixture's one non-declaration axiom actually made it
    // into the ontology, so an empty `loss` here is meaningful rather than
    // a stub that returns an empty vec without parsing anything.
    assert!(
        has_subclass_of(
            &loaded.ontology,
            "http://example.org/B",
            "http://example.org/A"
        ),
        "expected ex:B rdfs:subClassOf ex:A to be present, got {:#?}",
        loaded.ontology
    );
}

#[test]
fn missing_file_is_an_error_not_a_panic() {
    assert!(load_file(Path::new("tests/fixtures/nope.ttl")).is_err());
}

#[test]
fn merge_keeps_content_from_both_sides() {
    let mut base = load_file(Path::new("tests/fixtures/clean.ttl"))
        .expect("fixture should parse")
        .ontology;
    let other = load_file(Path::new("tests/fixtures/all-disjoint.ttl"))
        .expect("fixture should parse")
        .ontology;

    merge(&mut base, other);

    // From `base` (clean.ttl): the subclass axiom. If merge folded the
    // wrong direction (e.g. replaced base with other, or was a no-op on
    // other), this content would be missing.
    assert!(
        has_subclass_of(&base, "http://example.org/B", "http://example.org/A"),
        "merge lost content that was already in base: {:#?}",
        base
    );
    // From `other` (all-disjoint.ttl): ex:C is declared there and nowhere
    // in clean.ttl, so its presence proves `other` was actually folded in.
    assert!(
        declares_class(&base, "http://example.org/C"),
        "merge lost content from other: {:#?}",
        base
    );
}
