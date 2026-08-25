use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use sulo_testharness::cq::check_cq;
use sulo_testharness::load::{load_file, merge};
use sulo_testharness::manifest::CqSpec;
use sulo_testharness::materialize::materialize;
use sulo_testharness::prefixes::{base_mapping, with_overrides};
use sulo_testharness::verdict::Verdict;

const SULO: &str = "../sulo/sulo.ttl";
const FIXTURES: &str = "tests/fixtures";

/// Same fixture `tests/materialize.rs` uses: real SULO merged with
/// `tests/fixtures/parts.ttl`, then materialised into a queryable store.
fn parts_store() -> oxigraph::store::Store {
    let mut onto = load_file(Path::new(SULO)).unwrap().ontology;
    let data = load_file(Path::new("tests/fixtures/parts.ttl"))
        .unwrap()
        .ontology;
    merge(&mut onto, data);
    materialize(&onto, Duration::from_secs(30)).unwrap()
}

/// The prefix map every `expect_rows` token in this file resolves
/// against: the base map plus `ex:`, matching `parts.ttl`'s own
/// `@prefix ex:` binding.
fn pm() -> curie::PrefixMapping {
    let mut overrides = BTreeMap::new();
    overrides.insert("ex".to_string(), "http://example.org/".to_string());
    with_overrides(&base_mapping(), &overrides)
}

fn row(pairs: &[(&str, &str)]) -> BTreeMap<String, Option<String>> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), Some((*v).to_string())))
        .collect()
}

/// Like `row`, but each cell is an explicit `Option<&str>` so a test
/// can build a row with a `None` cell (YAML `null`, meaning "must be
/// unbound") alongside bound ones.
fn row_opt(pairs: &[(&str, Option<&str>)]) -> BTreeMap<String, Option<String>> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), v.map(|s| s.to_string())))
        .collect()
}

fn spec(
    query: &str,
    expect_rows: Vec<BTreeMap<String, Option<String>>>,
    exact: bool,
    ordered: bool,
) -> CqSpec {
    CqSpec {
        query: Path::new(query).to_path_buf(),
        expect_rows,
        exact,
        ordered,
    }
}

#[test]
fn a_matching_cq_passes() {
    // ex:a isPartOf ex:b (asserted), ex:a isPartOf ex:c (transitive),
    // ex:a isPartOf ex:a (reflexive self-loop): exactly these three.
    let store = parts_store();
    let s = spec(
        "queries/parts_of_a.rq",
        vec![
            row(&[("o", "ex:a")]),
            row(&[("o", "ex:b")]),
            row(&[("o", "ex:c")]),
        ],
        true,
        false,
    );
    let out = check_cq(&store, &s, Path::new(FIXTURES), &pm());
    assert_eq!(out.verdict, Verdict::Pass, "got {out:?}");
}

#[test]
fn a_mismatched_cq_fails_and_names_the_difference() {
    // ex:zzz is never a value of ex:a isPartOf ?o, so this must fail,
    // and the failure message must name the missing IRI.
    let store = parts_store();
    let s = spec(
        "queries/parts_of_a.rq",
        vec![row(&[("o", "ex:zzz")])],
        false,
        false,
    );
    let out = check_cq(&store, &s, Path::new(FIXTURES), &pm());
    match out.verdict {
        Verdict::Fail(msg) => {
            assert!(
                msg.contains("http://example.org/zzz"),
                "message should name the missing IRI, got: {msg}"
            );
        }
        other => panic!("expected Fail, got {other:?}"),
    }
}

#[test]
fn an_unparseable_query_is_indeterminate_not_a_fail() {
    // A broken .rq is a configuration error on the author's part, not
    // an ontology regression. Checks message content, not just the
    // variant: a bug that mislabelled this as an execute-time error
    // (or truncated the message) would still satisfy a bare
    // `Indeterminate(_)` match.
    let store = parts_store();
    let s = spec("queries/broken.rq", vec![], true, false);
    let out = check_cq(&store, &s, Path::new(FIXTURES), &pm());
    match out.verdict {
        Verdict::Indeterminate(reason) => {
            let msg = format!("{reason:?}");
            assert!(
                msg.contains("broken.rq") && msg.contains("parse"),
                "message should name the query file and say it could not be \
                 parsed, got: {msg}"
            );
        }
        other => panic!("expected Indeterminate, got {other:?}"),
    }
}

#[test]
fn a_missing_query_file_is_indeterminate_and_names_the_path() {
    let store = parts_store();
    let s = spec("queries/does-not-exist.rq", vec![], true, false);
    let out = check_cq(&store, &s, Path::new(FIXTURES), &pm());
    match out.verdict {
        Verdict::Indeterminate(reason) => {
            let msg = format!("{reason:?}");
            assert!(
                msg.contains("does-not-exist.rq"),
                "message should name the missing path, got: {msg}"
            );
        }
        other => panic!("expected Indeterminate, got {other:?}"),
    }
}

#[test]
fn an_ask_query_is_rejected_with_a_clear_message() {
    // expect_rows only makes sense for SELECT. An ASK must be a
    // configuration error rather than silently comparing zero rows.
    let store = parts_store();
    let s = spec("queries/ask_is_part_of.rq", vec![], true, false);
    let out = check_cq(&store, &s, Path::new(FIXTURES), &pm());
    match out.verdict {
        Verdict::Indeterminate(reason) => {
            let msg = format!("{reason:?}");
            assert!(
                msg.to_uppercase().contains("ASK"),
                "message should say this is an ASK query, got: {msg}"
            );
        }
        other => panic!("expected Indeterminate, got {other:?}"),
    }
}

#[test]
fn the_ordered_flag_reaches_the_comparator() {
    // queries/parts_of_a_desc.rq orders its results DESC(?o), so the
    // real answer comes back as c, b, a: the reverse of the ascending
    // order expect_rows declares below. With ordered: false the rows
    // must still match as a set; with ordered: true the same rows in
    // the same expect_rows order must fail, proving `ordered` actually
    // reaches `rows::compare` rather than being ignored.
    let store = parts_store();
    let expect_rows = vec![
        row(&[("o", "ex:a")]),
        row(&[("o", "ex:b")]),
        row(&[("o", "ex:c")]),
    ];

    let unordered = spec(
        "queries/parts_of_a_desc.rq",
        expect_rows.clone(),
        true,
        false,
    );
    let out = check_cq(&store, &unordered, Path::new(FIXTURES), &pm());
    assert_eq!(
        out.verdict,
        Verdict::Pass,
        "ordered: false must ignore the actual DESC order, got {out:?}"
    );

    let ordered = spec("queries/parts_of_a_desc.rq", expect_rows, true, true);
    let out = check_cq(&store, &ordered, Path::new(FIXTURES), &pm());
    match out.verdict {
        Verdict::Fail(_) => {}
        other => panic!(
            "ordered: true must reject the ASC expect_rows against the DESC actual order, got {other:?}"
        ),
    }
}

#[test]
fn an_unbound_variable_matches_expect_rows_null() {
    // queries/parts_of_a_optional_label.rq binds ?o the same way
    // parts_of_a.rq does (a, b, c), with an OPTIONAL ?label that never
    // binds: no ex: individual carries an rdfs:label. This is the
    // load-bearing case for the unbound-variable conversion in
    // check_cq: solution.get("label") returns None for every row, and
    // that must become an explicit None in the row map (matching a
    // YAML `null`), not a missing key. A conversion that iterated only
    // the solution's bound keys would drop "label" from the map
    // entirely and this row would then fail to compare equal to
    // expect_rows's explicit null.
    let store = parts_store();
    let s = spec(
        "queries/parts_of_a_optional_label.rq",
        vec![
            row_opt(&[("o", Some("ex:a")), ("label", None)]),
            row_opt(&[("o", Some("ex:b")), ("label", None)]),
            row_opt(&[("o", Some("ex:c")), ("label", None)]),
        ],
        true,
        false,
    );
    let out = check_cq(&store, &s, Path::new(FIXTURES), &pm());
    assert_eq!(out.verdict, Verdict::Pass, "got {out:?}");
}

#[test]
fn a_bad_expect_rows_token_is_indeterminate() {
    // "_:x" is a blank-node token, rejected by rows::parse_expected
    // because blank nodes never compare equal across runs. check_cq
    // must surface that as Indeterminate and name the offending token,
    // not silently drop the row or report it as an ordinary Fail.
    let store = parts_store();
    let s = spec(
        "queries/parts_of_a.rq",
        vec![row_opt(&[("o", Some("_:x"))])],
        true,
        false,
    );
    let out = check_cq(&store, &s, Path::new(FIXTURES), &pm());
    match out.verdict {
        Verdict::Indeterminate(reason) => {
            let msg = format!("{reason:?}");
            assert!(
                msg.contains("blank node 'x'"),
                "message should name the offending blank node token, got: {msg}"
            );
        }
        other => panic!("expected Indeterminate, got {other:?}"),
    }
}

#[test]
fn a_construct_query_is_rejected_with_a_clear_message() {
    // Same reasoning as the ASK rejection: expect_rows only applies to
    // SELECT, so a CONSTRUCT query must be a configuration error
    // rather than silently comparing zero rows.
    let store = parts_store();
    let s = spec("queries/construct_is_part_of.rq", vec![], true, false);
    let out = check_cq(&store, &s, Path::new(FIXTURES), &pm());
    match out.verdict {
        Verdict::Indeterminate(reason) => {
            let msg = format!("{reason:?}");
            assert!(
                msg.to_uppercase().contains("CONSTRUCT"),
                "message should say this is a CONSTRUCT/DESCRIBE query, got: {msg}"
            );
        }
        other => panic!("expected Indeterminate, got {other:?}"),
    }
}
