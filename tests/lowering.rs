use std::path::Path;
use sulo_testharness::load::load_file;

#[test]
fn covering_violation_is_inconsistent_after_lowering() {
    let loaded = load_file(Path::new("tests/fixtures/covering.ttl")).expect("fixture should parse");

    let consistent = owl_dl_reasoner::is_consistent(&loaded.ontology)
        .expect("consistency check should not error");

    // Without pre-lowering rustdl reports this consistent, which is
    // wrong: the disjoint union says A and B exhaust F.
    assert!(
        !consistent,
        "an F that is neither A nor B must be inconsistent once the \
         disjoint union is lowered"
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
