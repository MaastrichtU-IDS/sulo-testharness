use std::path::Path;
use sulo_testharness::load::load_file;

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
}

#[test]
fn a_clean_ontology_reports_no_loss() {
    let loaded = load_file(Path::new("tests/fixtures/clean.ttl")).expect("fixture should parse");
    assert!(loaded.loss.is_empty(), "unexpected loss: {:?}", loaded.loss);
}

#[test]
fn missing_file_is_an_error_not_a_panic() {
    assert!(load_file(Path::new("tests/fixtures/nope.ttl")).is_err());
}
