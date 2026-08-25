use std::path::Path;
use sulo_testharness::manifest::load_case;

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
    assert_eq!(c.cq.len(), 2, "both cq entries parsed");

    let first = &c.cq[0];
    assert_eq!(first.query, Path::new("queries/who.rq"));
    assert_eq!(first.expect_rows.len(), 2);
    assert_eq!(
        first.expect_rows[0].get("p"),
        Some(&Some("ex:alice".to_string()))
    );
    assert!(first.exact, "exact defaults to true");
    assert!(!first.ordered, "ordered defaults to false");

    let second = &c.cq[1];
    assert!(!second.exact, "exact was set false");
    assert!(second.ordered, "ordered was set true");
    assert_eq!(
        second.expect_rows[1].get("unit"),
        Some(&None),
        "a null in YAML means expected-unbound, not a missing key"
    );
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
