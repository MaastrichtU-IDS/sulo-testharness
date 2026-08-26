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

// ---------------------------------------------------------------
// `CheckOutcome::rests_on_absence`: which competency-question
// outcomes declare that their meaning depends on a row being ABSENT
// from the materialised store, and are therefore downgraded by
// `suite::downgrade_for_loss`. `check_cq` is the only place in the
// crate that computes this flag from anything other than a literal
// `false`, so the tests below are what stop it silently becoming a
// constant: one per disjunct that sets it (Fail, `exact: true`, a
// null cell, a non-monotone query) and one for the single
// combination that must leave it clear.
// ---------------------------------------------------------------

#[test]
fn a_cq_fail_declares_that_it_rests_on_absence() {
    // A Fail is always "this expected row was not there", which a
    // dropped axiom can produce just as well as an ontology
    // regression can. It carries no `oracle::NO_PROOF_MARKER`, so the
    // flag is the only signal `downgrade_for_loss` has.
    let store = parts_store();
    let s = spec(
        "queries/parts_of_a.rq",
        vec![row(&[("o", "ex:zzz")])],
        false,
        false,
    );
    let out = check_cq(&store, &s, Path::new(FIXTURES), &pm());
    assert!(matches!(out.verdict, Verdict::Fail(_)), "got {out:?}");
    assert!(
        out.rests_on_absence,
        "a CQ Fail rests on a row's absence and must say so"
    );
}

#[test]
fn an_exact_cq_pass_declares_that_it_rests_on_absence() {
    // `exact: true` asserts "and no other rows", which is an absence
    // claim: under axiom loss the closure is a subset of the intended
    // one, so a suppressed extra row makes it pass unearned.
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
    assert!(
        out.rests_on_absence,
        "exact: true is an absence claim and must say so"
    );
}

#[test]
fn a_cq_pass_with_a_null_cell_declares_that_it_rests_on_absence() {
    // A `null` cell asserts "this variable is unbound", which is
    // again an absence claim: a binding the reasoner would have
    // inferred from a dropped axiom would have refuted it.
    //
    // The query is the UNION one, not the OPTIONAL one this test used
    // before `query_is_monotone` existed. OPTIONAL is on the
    // non-monotone keyword list, so over that query the flag would be
    // set by TWO disjuncts at once and this test could no longer fail
    // if the null-cell disjunct were deleted: a check that cannot
    // fail, introduced by the very change that added the third
    // disjunct. The UNION query is monotone and still projects an
    // unbound cell, so the null cell is the only reason left.
    let store = parts_store();
    let s = spec(
        "queries/parts_of_a_union_label.rq",
        vec![
            row_opt(&[("o", Some("ex:a")), ("label", None)]),
            row_opt(&[("o", Some("ex:b")), ("label", None)]),
        ],
        false,
        false,
    );
    let out = check_cq(&store, &s, Path::new(FIXTURES), &pm());
    assert_eq!(out.verdict, Verdict::Pass, "got {out:?}");
    assert!(
        out.rests_on_absence,
        "a null cell is an unboundedness claim and must say so"
    );
}

#[test]
fn a_subset_cq_pass_over_a_monotone_query_with_every_cell_bound_is_not_flagged() {
    // The polarity that must NOT be flagged: `exact: false` with no
    // null cell asserts only that certain rows are PRESENT, and over
    // a MONOTONE query (this one is a single basic graph pattern:
    // no window, no aggregate, no negation) a smaller store can only
    // remove rows. Flagging it would downgrade a trustworthy Pass to
    // Indeterminate for no reason, which overstates the harness's
    // uncertainty in the other direction.
    //
    // The name of this test used to be the claim itself
    // ("..._is_monotone_safe"), stated for every query rather than
    // for a monotone one; the two tests below are the cases that
    // claim was false for.
    let store = parts_store();
    let s = spec(
        "queries/parts_of_a.rq",
        vec![row(&[("o", "ex:b")])],
        false,
        false,
    );
    let out = check_cq(&store, &s, Path::new(FIXTURES), &pm());
    assert_eq!(out.verdict, Verdict::Pass, "got {out:?}");
    assert!(
        !out.rests_on_absence,
        "a subset Pass with every cell bound over a monotone query must not be flagged"
    );
}

#[test]
fn a_subset_cq_pass_over_a_limit_query_declares_that_it_rests_on_absence() {
    // Same spec shape as the test above, `exact: false` with every
    // cell bound, over a query with ORDER BY + LIMIT. Loss does not
    // merely remove rows here: dropping ex:a would promote ex:c into
    // the two-row window, so this Pass could be MANUFACTURED by loss
    // and must declare itself.
    let store = parts_store();
    let s = spec(
        "queries/parts_of_a_limit.rq",
        vec![row(&[("o", "ex:a")])],
        false,
        false,
    );
    let out = check_cq(&store, &s, Path::new(FIXTURES), &pm());
    assert_eq!(out.verdict, Verdict::Pass, "got {out:?}");
    assert!(
        out.rests_on_absence,
        "a LIMIT query is non-monotone: a smaller store can promote a row into \
         the window, so this Pass must declare that it rests on absence"
    );
}

#[test]
fn a_subset_cq_pass_over_an_aggregate_query_declares_that_it_rests_on_absence() {
    // The second non-monotone mechanism: COUNT emits a row whatever
    // the store holds, so loss changes the VALUE rather than removing
    // the row, and an expected count can be hit by losing an axiom
    // just as well as by the ontology being right.
    let store = parts_store();
    let s = spec(
        "queries/parts_of_a_count.rq",
        vec![row(&[("n", "\"3\"^^xsd:integer")])],
        false,
        false,
    );
    let out = check_cq(&store, &s, Path::new(FIXTURES), &pm());
    assert_eq!(out.verdict, Verdict::Pass, "got {out:?}");
    assert!(
        out.rests_on_absence,
        "an aggregate query is non-monotone: loss changes the aggregated value, \
         so this Pass must declare that it rests on absence"
    );
}

// ---------------------------------------------------------------
// Spec 7.3: `ordered: true` is only valid with an `ORDER BY` in the
// query. Not decidable from the manifest (the manifest does not hold
// the query text), so it is `check_cq`'s job, not `load_case`'s.
// ---------------------------------------------------------------

#[test]
fn ordered_true_over_a_query_without_order_by_is_indeterminate() {
    // Without ORDER BY, SPARQL leaves the row order arbitrary, so
    // whether the sequence comparison passes is a coin flip. Refuse
    // to report a verdict rather than report a flaky one.
    let store = parts_store();
    let s = spec(
        "queries/parts_of_a.rq",
        vec![
            row(&[("o", "ex:a")]),
            row(&[("o", "ex:b")]),
            row(&[("o", "ex:c")]),
        ],
        true,
        true,
    );
    let out = check_cq(&store, &s, Path::new(FIXTURES), &pm());
    match out.verdict {
        Verdict::Indeterminate(reason) => {
            let msg = format!("{reason:?}");
            assert!(
                msg.contains("no ORDER BY") && msg.contains("ordered: true"),
                "message should name the missing ORDER BY and the setting that \
                 needs it, got: {msg}"
            );
        }
        other => panic!("expected Indeterminate, got {other:?}"),
    }
}

#[test]
fn ordered_true_over_a_query_with_order_by_is_compared_normally() {
    // The other direction, or the guard above could be a check that
    // always fires: the same `ordered: true` spec over a query that
    // does have ORDER BY reaches the comparison and gets a real
    // verdict. `parts_of_a_desc.rq` sorts descending, so this is the
    // reverse of the row order above.
    let store = parts_store();
    let s = spec(
        "queries/parts_of_a_desc.rq",
        vec![
            row(&[("o", "ex:c")]),
            row(&[("o", "ex:b")]),
            row(&[("o", "ex:a")]),
        ],
        true,
        true,
    );
    let out = check_cq(&store, &s, Path::new(FIXTURES), &pm());
    assert_eq!(out.verdict, Verdict::Pass, "got {out:?}");
}
