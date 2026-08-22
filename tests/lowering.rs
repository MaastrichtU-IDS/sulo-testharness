use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use horned_owl::io::ParserConfiguration;
use horned_owl::io::rdf::reader::read as read_rdf;
use horned_owl::model::{
    Build, ClassExpression, Component, DisjointClasses, DisjointUnion, EquivalentClasses, RcStr,
};
use horned_owl::ontology::set::SetOntology;
use sulo_testharness::load::{load_file, lower_disjoint_unions};

#[test]
fn covering_violation_is_inconsistent() {
    let loaded = load_file(Path::new("tests/fixtures/covering.ttl")).expect("fixture should parse");

    let consistent = owl_dl_reasoner::is_consistent(&loaded.ontology)
        .expect("consistency check should not error");

    // NOTE: this passes with or without lower_disjoint_unions at the
    // pinned owl-dl-reasoner v0.4.22 -- that version already enforces
    // the covering half of DisjointUnion natively (confirmed by
    // disabling the lowering and re-running: still inconsistent). The
    // lowering exists so this keeps passing on a reasoner build that
    // does NOT handle DisjointUnion natively, which is a real,
    // measured failure mode of a later rustdl working-tree build. See
    // the doc comment on lower_disjoint_unions in src/load.rs.
    assert!(
        !consistent,
        "an F that is neither A nor B must be inconsistent: A and B \
         exhaust F under the disjoint union"
    );
}

#[test]
fn lowering_preserves_the_disjointness_half() {
    // An individual in both A and B must still clash.
    let loaded =
        load_file(Path::new("tests/fixtures/covering-both.ttl")).expect("fixture should parse");
    let consistent = owl_dl_reasoner::is_consistent(&loaded.ontology).unwrap();
    assert!(
        !consistent,
        "A and B are disjoint, so being both must clash"
    );
}

// Positive control (Ruling I): both tests above assert `!consistent`,
// so an `is_consistent` that always returns `false`, or a
// `lower_disjoint_unions` that corrupts the ontology into blanket
// unsatisfiability, would pass both while proving nothing. This test
// uses the same lowering on a fixture with no contradiction and
// requires it to stay consistent, which only a correct lowering (and
// a correct is_consistent) can satisfy.
#[test]
fn lowering_a_consistent_ontology_stays_consistent() {
    let loaded =
        load_file(Path::new("tests/fixtures/covering-ok.ttl")).expect("fixture should parse");
    let consistent = owl_dl_reasoner::is_consistent(&loaded.ontology).unwrap();
    assert!(
        consistent,
        "an F asserted to be neither both A and B nor neither A nor B \
         has no contradiction and must remain consistent after lowering"
    );
}

/// Exercises `lower_disjoint_unions` directly, independent of any
/// reasoner behaviour. The three tests above only observe
/// `is_consistent`'s output, which (as demonstrated separately) still
/// passes even when `lower_disjoint_unions` is stubbed to a no-op at
/// the pinned reasoner version -- so none of them actually prove the
/// function does anything. This test asserts on the ontology's
/// components directly and must fail against a no-op.
#[test]
fn lower_disjoint_unions_rewrites_into_equivalent_and_disjoint_classes() {
    let file = File::open("tests/fixtures/covering.ttl").expect("fixture should open");
    let mut reader = BufReader::new(file);
    let mut config = ParserConfiguration::default();
    config.rdf.format = Some(oxrdfio::RdfFormat::Turtle);
    let (concrete, _incomplete) = read_rdf(&mut reader, config).expect("fixture should parse");
    let mut onto: SetOntology<RcStr> = concrete.into();

    // covering.ttl has exactly one DisjointUnion(F, [A, B]).
    let count = lower_disjoint_unions(&mut onto);
    assert_eq!(
        count, 1,
        "expected exactly one DisjointUnion to be rewritten"
    );

    let b: Build<RcStr> = Build::new_rc();
    let f = b.class("http://example.org/F");
    let a = b.class("http://example.org/A");
    let bb = b.class("http://example.org/B");

    let expected_equiv = Component::EquivalentClasses(EquivalentClasses(vec![
        ClassExpression::Class(f.clone()),
        ClassExpression::ObjectUnionOf(vec![
            ClassExpression::Class(a.clone()),
            ClassExpression::Class(bb.clone()),
        ]),
    ]));
    assert!(
        onto.iter().any(|ac| ac.component == expected_equiv),
        "expected EquivalentClasses(F, ObjectUnionOf(A, B)) after lowering, got {:#?}",
        onto
    );

    let expected_disjoint = Component::DisjointClasses(DisjointClasses(vec![
        ClassExpression::Class(a.clone()),
        ClassExpression::Class(bb.clone()),
    ]));
    assert!(
        onto.iter().any(|ac| ac.component == expected_disjoint),
        "expected DisjointClasses(A, B) after lowering, got {:#?}",
        onto
    );

    // The original DisjointUnion must still be present: lowering adds,
    // it does not replace.
    let expected_du = Component::DisjointUnion(DisjointUnion(
        f,
        vec![ClassExpression::Class(a), ClassExpression::Class(bb)],
    ));
    assert!(
        onto.iter().any(|ac| ac.component == expected_du),
        "expected the original DisjointUnion to remain, got {:#?}",
        onto
    );
}
