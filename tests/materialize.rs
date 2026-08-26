use std::path::Path;
use std::time::Duration;

use oxigraph::sparql::{QueryResults, SparqlEvaluator};
use sulo_testharness::load::{load_file, merge};
use sulo_testharness::materialize::materialize;

const SULO: &str = "../sulo/sulo.ttl";

fn ask(store: &oxigraph::store::Store, q: &str) -> bool {
    let r = SparqlEvaluator::new()
        .parse_query(q)
        .unwrap()
        .on_store(store)
        .execute()
        .unwrap();
    match r {
        QueryResults::Boolean(b) => b,
        _ => panic!("expected an ASK"),
    }
}

fn parts_store() -> oxigraph::store::Store {
    let mut onto = load_file(Path::new(SULO)).unwrap().ontology;
    let data = load_file(Path::new("tests/fixtures/parts.ttl"))
        .unwrap()
        .ontology;
    merge(&mut onto, data);
    materialize(&onto, Duration::from_secs(30)).unwrap()
}

#[test]
fn asserted_triples_are_present() {
    let s = parts_store();
    assert!(ask(
        &s,
        "ASK { <http://example.org/a> <https://w3id.org/sulo/isPartOf> <http://example.org/b> }"
    ));
}

#[test]
fn inferred_transitive_closure_is_present() {
    let s = parts_store();
    assert!(
        ask(
            &s,
            "ASK { <http://example.org/a> <https://w3id.org/sulo/isPartOf> <http://example.org/c> }"
        ),
        "isPartOf is transitive, so a isPartOf c must be materialised"
    );
}

#[test]
fn inferred_subproperty_propagation_is_present() {
    let s = parts_store();
    assert!(
        ask(
            &s,
            "ASK { <http://example.org/a> <https://w3id.org/sulo/isIn> <http://example.org/c> }"
        ),
        "isPartOf is a subproperty of isIn"
    );
}

#[test]
fn reflexive_self_loops_are_injected() {
    // inferred_object_property_values omits these, so without explicit
    // injection a CQ pattern ?x sulo:isPartOf ?x silently returns
    // nothing despite isPartOf being reflexive. Spec section 8 step 6.
    let s = parts_store();
    assert!(
        ask(
            &s,
            "ASK { <http://example.org/d> <https://w3id.org/sulo/isPartOf> <http://example.org/d> }"
        ),
        "reflexive self-loop must be injected for every named individual"
    );
}

#[test]
fn inferred_class_assertions_use_the_full_closure() {
    // ex:d is asserted SpatialObject; Object is an ancestor.
    let s = parts_store();
    assert!(
        ask(
            &s,
            "ASK { <http://example.org/d> a <https://w3id.org/sulo/Object> }"
        ),
        "class assertions must be the full closure, not most-specific types"
    );
}

#[test]
fn a_non_entailment_is_absent() {
    // The store must not contain everything: a false statement must
    // be absent, or every CQ would pass.
    let s = parts_store();
    assert!(
        !ask(
            &s,
            "ASK { <http://example.org/d> a <https://w3id.org/sulo/Process> }"
        ),
        "ex:d is a SpatialObject, which is disjoint from Process"
    );
}

#[test]
fn a_zero_deadline_times_out_rather_than_hanging() {
    let mut onto = load_file(Path::new(SULO)).unwrap().ontology;
    let data = load_file(Path::new("tests/fixtures/parts.ttl"))
        .unwrap()
        .ontology;
    merge(&mut onto, data);
    let r = materialize(&onto, Duration::from_millis(0));
    assert!(r.is_err(), "a zero deadline must not silently succeed");
}

#[test]
fn a_typed_data_property_value_is_present() {
    let s = parts_store();
    assert!(
        ask(
            &s,
            r#"ASK { <http://example.org/o> <https://w3id.org/sulo/hasValue> "5"^^<http://www.w3.org/2001/XMLSchema#int> }"#
        ),
        "ex:o hasValue \"5\"^^xsd:int must be present as an exact typed literal"
    );
}

#[test]
fn a_plain_data_property_value_is_present() {
    let s = parts_store();
    assert!(
        ask(
            &s,
            r#"ASK { <http://example.org/m> <https://w3id.org/sulo/hasValue> "hello" }"#
        ),
        "ex:m hasValue \"hello\" must be present as an exact xsd:string-typed literal"
    );
}

#[test]
fn a_language_tagged_data_property_value_is_present() {
    let s = parts_store();
    assert!(
        ask(
            &s,
            r#"ASK { <http://example.org/n> <https://w3id.org/sulo/hasValue> "bonjour"@fr }"#
        ),
        "ex:n hasValue \"bonjour\"@fr must be present with its language tag intact"
    );
}

#[test]
fn a_language_tagged_value_never_becomes_a_tagless_langstring() {
    // Regression for MaastrichtU-IDS/rustdl#72: inferred_data_property_values
    // drops the `lang` element of its 5-tuple, so naively rebuilding every
    // data value as `Literal::new_typed_literal(lexical, datatype)` off that
    // 4-tuple silently reconstructs "bonjour"@fr as a TAGLESS
    // rdf:langString-typed literal: not RDF-1.1-well-formed, and not equal
    // to the correct term. materialize() must use the 5-tuple form
    // (`materialize_data_property_assertions`) directly instead, so this
    // malformed term must never appear in the store, alongside the correct
    // one asserted by `a_language_tagged_data_property_value_is_present`.
    let s = parts_store();
    assert!(
        !ask(
            &s,
            r#"ASK { <http://example.org/n> <https://w3id.org/sulo/hasValue> "bonjour"^^<http://www.w3.org/1999/02/22-rdf-syntax-ns#langString> }"#
        ),
        "a tagless rdf:langString duplicate of a language-tagged value must never be inserted"
    );
}
