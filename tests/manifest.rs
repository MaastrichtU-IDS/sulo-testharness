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
