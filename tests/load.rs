use std::path::Path;

use horned_owl::model::{
    Build, ClassExpression, Component, DeclareClass, DisjointClasses, RcStr, SubClassOf,
};
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

/// True if `onto` contains a `DisjointClasses` axiom over exactly this
/// set of IRIs (order-insensitive: `owl:AllDisjointClasses` carries no
/// order, and `recover_all_disjoint_classes` does not promise to
/// preserve the source list's order either).
fn has_disjoint_classes(onto: &SetOntology<RcStr>, iris: &[&str]) -> bool {
    use std::collections::BTreeSet;
    let b: Build<RcStr> = Build::new_rc();
    let expected: BTreeSet<ClassExpression<RcStr>> = iris
        .iter()
        .map(|iri| ClassExpression::Class(b.class(*iri)))
        .collect();
    onto.iter().any(|ac| match &ac.component {
        Component::DisjointClasses(DisjointClasses(members)) => {
            members.iter().cloned().collect::<BTreeSet<_>>() == expected
        }
        _ => false,
    })
}

#[test]
fn recovers_alldisjointclasses_from_incomplete_parse_leftovers() {
    // horned-owl has no vocabulary entry for owl:AllDisjointClasses, so
    // its triples land in IncompleteParse. `load_file` reconstructs the
    // axiom from those leftovers, so this must now be loss-free, unlike
    // before recovery existed (see the adjacent
    // `alldisjointproperties_is_still_genuinely_dropped_as_loss`, which
    // keeps this fixture's original intent alive for a construct
    // recovery does not target).
    let loaded =
        load_file(Path::new("tests/fixtures/all-disjoint.ttl")).expect("fixture should parse");

    assert!(
        loaded.loss.is_empty(),
        "owl:AllDisjointClasses should be recovered, not reported as loss: {:?}",
        loaded.loss
    );
    assert!(
        has_disjoint_classes(
            &loaded.ontology,
            &[
                "http://example.org/A",
                "http://example.org/B",
                "http://example.org/C"
            ]
        ),
        "expected a recovered DisjointClasses(A, B, C) axiom, got {:#?}",
        loaded.ontology
    );

    // The three class declarations precede the AllDisjointClasses triples
    // and are ordinary axioms the reader does handle. If they are absent,
    // no real parsing happened and the assertions above would be
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

/// Adapted from Task 2's original `owl:AllDisjointClasses` loss test,
/// which the recovery above made vacuous (that construct is no longer
/// loss). `owl:AllDisjointProperties` is the same shape
/// (`[] a owl:AllDisjointProperties ; owl:members (...)`) but horned-owl
/// has no vocabulary entry for it either, and `recover_all_disjoint_classes`
/// deliberately does not target it (out of scope for this task; see
/// `mutants/README.md`), so it stays genuinely, permanently dropped.
/// This keeps Task 2's intent (the loader must surface what horned-owl
/// cannot parse, not swallow it) meaningful against a construct that is
/// still true of the current code.
#[test]
fn alldisjointproperties_is_still_genuinely_dropped_as_loss() {
    let loaded = load_file(Path::new("tests/fixtures/all-disjoint-properties.ttl"))
        .expect("fixture should parse");

    assert!(
        !loaded.loss.is_empty(),
        "AllDisjointProperties must be reported as loss, got none"
    );
    assert!(
        loaded.loss.iter().any(|d| d.contains("parse")),
        "loss should name the parse channel, got {:?}",
        loaded.loss
    );

    // The three property declarations are ordinary axioms the reader
    // does handle; if absent, no real parsing happened and the loss
    // assertions above would be vacuously true for a stub.
    for iri in [
        "http://example.org/p",
        "http://example.org/q",
        "http://example.org/r",
    ] {
        assert!(
            loaded.ontology.iter().any(|ac| {
                let b: Build<RcStr> = Build::new_rc();
                ac.component
                    == Component::DeclareObjectProperty(horned_owl::model::DeclareObjectProperty(
                        b.object_property(iri),
                    ))
            }),
            "expected {iri} to be declared an object property, got {:#?}",
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

/// Regression test for fix round 2, CRITICAL 1: the known-baseline
/// check must anchor on which axioms were actually dropped, not just
/// on the aggregate (kind, count) shape. `unrelated-data-range-drops.ttl`
/// drops exactly two `SubClassOf` axioms of the exact same kind real
/// SULO's known baseline uses, on classes (`ex:Foo`, `ex:Bar`) that
/// have nothing to do with `sulo:TimeInstant`/`sulo:InformationObject`.
/// Before `has_known_baseline_axioms` was added (fix round 1's
/// `is_known_baseline` checked only `kinds_map.len() == 1 &&
/// kinds_map.get(KIND) == Some(&COUNT)`), this fixture's loss was
/// shape-identical to the real baseline and was silently exempted:
/// confirmed by inspecting that exact code, which has no reference to
/// which file or which axioms were involved. This must now fail that
/// way: the loss must land in `loss` (and downgrade verdicts), not in
/// `baseline_loss`.
#[test]
fn a_shape_identical_but_unrelated_drop_is_not_exempted_as_baseline() {
    let loaded = load_file(Path::new("tests/fixtures/unrelated-data-range-drops.ttl"))
        .expect("fixture should parse");

    assert!(
        !loaded.loss.is_empty(),
        "an unrelated drop that merely matches the baseline's kind and count \
         must still be reported as loss, got none"
    );
    assert!(
        loaded.baseline_loss.is_empty(),
        "an unrelated drop must never be folded into baseline_loss just because \
         its kind and count coincide with the real baseline, got {:?}",
        loaded.baseline_loss
    );
}
