use std::path::Path;
use sulo_testharness::manifest::{ManifestError, load_case};

#[test]
fn parses_a_well_formed_case() {
    let c = load_case(Path::new("tests/fixtures/case-ok.yaml")).unwrap();
    assert_eq!(c.id, "pro-role-chain");
    assert_eq!(c.data.len(), 1);
    assert_eq!(c.prefixes.get("ex").unwrap(), "http://example.org/");
    assert!(c.entails.is_some());
    assert_eq!(c.entails_manchester.len(), 1);
    assert_eq!(c.entails_manchester[0].sub_expr, "sulo:Feature");
    assert_eq!(c.tags, vec!["pattern", "pro"]);
    assert_eq!(c.timeout_ms, 30_000, "default timeout");
    assert!(!c.expect_inconsistent, "default is expecting consistency");
    assert_eq!(c.base_dir, Path::new("tests/fixtures"));
}

#[test]
fn a_single_data_path_is_accepted_as_a_string() {
    let c = load_case(Path::new("tests/fixtures/case-ok.yaml")).unwrap();
    assert_eq!(c.data[0].to_str().unwrap(), "data/pro-encounter.ttl");
}

#[test]
fn an_unknown_key_is_rejected_loudly() {
    // A typo like `entials:` must not silently mean "no entailments to check".
    let err = load_case(Path::new("tests/fixtures/case-bad-key.yaml"))
        .expect_err("unknown key must be an error");
    assert!(
        err.to_string().contains("entials"),
        "the error should name the offending key, got: {err}"
    );
}

#[test]
fn a_case_that_asserts_nothing_is_rejected() {
    // `deny_unknown_fields` catches `entials:`; this catches the other
    // way to arrive at the same place, a manifest with only `id` and
    // `description`. Such a case parses, pushes zero checks, and
    // `aggregate` returns Pass over the empty set: a green for a test
    // that tests nothing, which manifest.rs's own module doc calls the
    // single worst failure mode available to a test harness.
    let err = load_case(Path::new("tests/fixtures/case-no-assertions.yaml"))
        .expect_err("a case with no assertion field must be an error");
    let msg = err.to_string();
    assert!(
        msg.contains("asserts nothing"),
        "the error should say the case asserts nothing, got: {msg}"
    );
    assert!(
        msg.contains("entails") && msg.contains("expect_inconsistent"),
        "the error should list the fields that would make it a real case, got: {msg}"
    );
}

#[test]
fn expect_inconsistent_alone_is_a_real_case() {
    // The consistency gate IS the assertion when
    // `expect_inconsistent: true`, so such a case must survive the
    // no-assertions check. Guards against the fix over-rejecting.
    let c = load_case(Path::new(
        "tests/fixtures/case-expect-inconsistent-only.yaml",
    ))
    .expect("expect_inconsistent alone is a complete case");
    assert!(c.expect_inconsistent);
    assert!(c.entails.is_none());
}

#[test]
fn parses_the_cq_block() {
    let c = load_case(Path::new("tests/fixtures/case-with-cq.yaml")).unwrap();
    assert_eq!(c.cq.len(), 3, "every cq entry parsed");

    let first = &c.cq[0];
    assert_eq!(first.query, Path::new("queries/who.rq"));
    assert_eq!(first.expect_rows.len(), 2);
    assert_eq!(
        first.expect_rows[0].get("p"),
        Some(&Some("ex:alice".to_string()))
    );
    assert!(first.exact, "exact defaults to true");
    assert!(!first.ordered, "ordered defaults to false");

    // The non-default `ordered: true` corner, which the schema only
    // permits alongside `exact: true` (spec 7.3; load_case refuses
    // the other combination, see the two tests below), plus the
    // `null` cell.
    let second = &c.cq[1];
    // This assertion cannot tell an explicit `exact: true` from the
    // key being absent, because `true` is also the default: it pins
    // the effective value, not the parse of a non-default one. The
    // third entry below carries the real non-default coverage
    // (`exact: false`, which only an explicit key can produce).
    assert!(
        second.exact,
        "exact is true, explicitly here and by default"
    );
    assert!(second.ordered, "ordered was set true");
    assert_eq!(
        second.expect_rows[1].get("unit"),
        Some(&None),
        "a null in YAML means expected-unbound, not a missing key"
    );

    // The non-default `exact: false` corner, which needs a non-empty
    // expect_rows to assert anything at all.
    let third = &c.cq[2];
    assert!(!third.exact, "exact was set false");
    assert!(!third.ordered, "ordered was set false");
    assert_eq!(third.expect_rows.len(), 1);
}

#[test]
fn a_cq_with_ordered_true_and_exact_false_is_rejected_at_load() {
    // Statically decidable from the manifest alone, so it is refused
    // here (exit 2, a configuration error) rather than at check time.
    // Before this guard, `rows::compare`'s refusal reached
    // `check_cq`, which mapped every Err to `Verdict::Fail`, and a
    // YAML typo was reported as an ontology regression.
    let err = load_case(Path::new("tests/fixtures/case-cq-ordered-not-exact.yaml"))
        .expect_err("ordered: true with exact: false must be refused at load");
    assert!(
        matches!(err, ManifestError::CqOrderedNotExact { .. }),
        "expected CqOrderedNotExact, got {err:?}"
    );
    let msg = err.to_string();
    assert!(
        msg.contains("queries/values.rq"),
        "the error should name the offending query, got: {msg}"
    );
    assert!(
        msg.contains("ordered: true") && msg.contains("exact: false"),
        "the error should name both flags, got: {msg}"
    );
    assert!(
        msg.contains("7.3"),
        "the error should cite the spec section that leaves it undefined, got: {msg}"
    );
}

#[test]
fn a_cq_with_empty_expect_rows_and_exact_false_is_rejected_at_load() {
    // `rows::compare` with an empty `expected` and `exact: false`
    // runs an empty loop, skips the leftover check, and returns
    // Ok(()) whatever the query returned: a check that cannot fail,
    // the exact shape `NoAssertions` exists to refuse, one level in.
    let err = load_case(Path::new(
        "tests/fixtures/case-cq-empty-rows-not-exact.yaml",
    ))
    .expect_err("an empty expect_rows with exact: false must be refused at load");
    assert!(
        matches!(err, ManifestError::CqAssertsNothing { .. }),
        "expected CqAssertsNothing, got {err:?}"
    );
    let msg = err.to_string();
    assert!(
        msg.contains("queries/who.rq"),
        "the error should name the offending query, got: {msg}"
    );
    assert!(
        msg.contains("asserts nothing"),
        "the error should say the entry asserts nothing, got: {msg}"
    );
    assert!(
        msg.contains("exact: true"),
        "the error should point at the legitimate empty-result form, got: {msg}"
    );
}

#[test]
fn a_cq_with_empty_expect_rows_and_exact_true_still_loads() {
    // Guards the refusal above against over-rejecting: "this query
    // must return nothing" is a real assertion, and `rows::compare`
    // enforces it through the leftover check that `exact: true`
    // switches on.
    let c = load_case(Path::new("tests/fixtures/case-cq-empty-rows-exact.yaml"))
        .expect("an empty expect_rows with exact: true is a real assertion");
    assert_eq!(c.cq.len(), 1);
    assert!(c.cq[0].expect_rows.is_empty());
    assert!(c.cq[0].exact);
}

#[test]
fn a_case_with_only_a_cq_is_a_real_case() {
    // `cq` must satisfy the no-assertions guard from the engine plan,
    // otherwise a pure competency-question case is rejected.
    let c = load_case(Path::new("tests/fixtures/case-with-cq.yaml")).unwrap();
    assert!(!c.cq.is_empty());
}

#[test]
fn an_unknown_key_inside_cq_is_rejected() {
    // Same reasoning as the top-level deny_unknown_fields: a typo'd
    // `expect_row:` must not silently mean "no rows expected".
    let err = load_case(Path::new("tests/fixtures/case-cq-bad-key.yaml"))
        .expect_err("unknown key inside a cq entry must be an error");
    assert!(
        err.to_string().contains("expect_row"),
        "error names the key: {err}"
    );
}
