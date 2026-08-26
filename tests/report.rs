//! `report::render`, the surface where the harness's honesty is
//! actually delivered to a reader.
//!
//! Two groups of tests. The first is the baseline-loss preamble (fix
//! round 2's "cheap item"): a CI consumer reading only the rendered
//! report, not the console, must still be able to see that a run
//! carried known, permanent, pinned-reasoner loss. The second covers
//! what spec 5.1 actually requires of the output and what was
//! previously untested altogether: unrefuted passes counted and
//! reported SEPARATELY from verified passes, the four-way verdict tag
//! mapping, the skip notice, and the per-check Fail and Indeterminate
//! message lines. Before these, deleting the `unrefuted += 1`
//! increment or collapsing the `PASS*` tag to `PASS` left every test
//! in the repository green.

use sulo_testharness::report::{render, render_json, render_junit};
use sulo_testharness::suite::{CaseResult, DeferredCase};
use sulo_testharness::verdict::{CheckOutcome, IndeterminateReason, Verdict};

fn passing_case(id: &str, baseline_loss: Vec<String>) -> CaseResult {
    CaseResult {
        id: id.into(),
        verdict: Verdict::Pass,
        checks: vec![],
        skipped: false,
        baseline_loss,
    }
}

#[test]
fn baseline_loss_appears_as_a_report_preamble() {
    let results = vec![passing_case(
        "c1",
        vec!["conversion: 2 dropped (SubClassOf: unsupported data range x2)".into()],
    )];

    let out = render(&results, &[]);

    assert!(
        out.contains("known baseline loss"),
        "expected a baseline-loss preamble, got: {out}"
    );
    assert!(
        out.contains("SubClassOf: unsupported data range x2"),
        "expected the actual baseline message to appear verbatim, got: {out}"
    );
}

#[test]
fn no_baseline_loss_means_no_preamble() {
    let results = vec![passing_case("c1", vec![])];

    let out = render(&results, &[]);

    assert!(
        !out.contains("known baseline loss"),
        "no case carried baseline loss, so no preamble should appear, got: {out}"
    );
}

#[test]
fn baseline_loss_is_deduplicated_across_cases() {
    let msg = "conversion: 2 dropped (SubClassOf: unsupported data range x2)".to_string();
    let results = vec![
        passing_case("c1", vec![msg.clone()]),
        passing_case("c2", vec![msg.clone()]),
    ];

    let out = render(&results, &[]);
    let occurrences = out.matches("SubClassOf: unsupported data range x2").count();
    assert_eq!(
        occurrences, 1,
        "the same baseline message from two cases should be deduplicated, got: {out}"
    );
}

fn case_with(id: &str, verdict: Verdict, checks: Vec<CheckOutcome>, skipped: bool) -> CaseResult {
    CaseResult {
        id: id.into(),
        verdict,
        checks,
        skipped,
        baseline_loss: vec![],
    }
}

fn check(name: &str, verdict: Verdict) -> CheckOutcome {
    CheckOutcome {
        name: name.into(),
        verdict,
        rests_on_absence: false,
    }
}

#[test]
fn unrefuted_passes_are_counted_and_reported_separately() {
    // Spec 5.1: a negative expectation the reasoner failed to refute
    // is not a verified pass, and the report must not let a reader
    // mistake one for the other. Two unrefuted checks across two
    // cases, plus one verified Pass that must NOT be counted.
    let results = vec![
        case_with(
            "c1",
            Verdict::UnrefutedPass,
            vec![
                check("neg", Verdict::UnrefutedPass),
                check("pos", Verdict::Pass),
            ],
            false,
        ),
        case_with(
            "c2",
            Verdict::UnrefutedPass,
            vec![check("neg", Verdict::UnrefutedPass)],
            false,
        ),
    ];

    let out = render(&results, &[]);

    assert!(
        out.contains("2 check(s) marked PASS*"),
        "both unrefuted checks must be counted, and the verified Pass must not \
         be counted among them, got: {out}"
    );
    assert!(
        out.contains("not a proof of non-entailment"),
        "the summary must say what PASS* means, got: {out}"
    );
    assert!(
        out.contains("PASS*  c1") && out.contains("PASS*  c2"),
        "an unrefuted case must be tagged PASS*, distinctly from PASS, got: {out}"
    );
}

#[test]
fn no_unrefuted_checks_means_no_unrefuted_summary() {
    let results = vec![case_with(
        "c1",
        Verdict::Pass,
        vec![check("pos", Verdict::Pass)],
        false,
    )];
    let out = render(&results, &[]);
    assert!(
        !out.contains("PASS*"),
        "with nothing unrefuted, neither the tag nor the summary should appear, got: {out}"
    );
}

#[test]
fn the_four_verdicts_map_to_four_distinct_tags() {
    // A four-way mapping collapsed to three (the likeliest edit) would
    // erase exactly the distinction the harness exists to make.
    let results = vec![
        case_with("pass-case", Verdict::Pass, vec![], false),
        case_with("unrefuted-case", Verdict::UnrefutedPass, vec![], false),
        case_with(
            "indet-case",
            Verdict::Indeterminate(IndeterminateReason::Timeout),
            vec![],
            false,
        ),
        case_with("fail-case", Verdict::Fail("boom".into()), vec![], false),
    ];

    let out = render(&results, &[]);
    let tags: Vec<&str> = out
        .lines()
        .filter_map(|l| l.split_whitespace().next())
        .collect();

    assert_eq!(
        tags,
        vec!["PASS", "PASS*", "INDET", "FAIL"],
        "each verdict must render as its own tag, got: {out}"
    );
}

#[test]
fn fail_and_indeterminate_check_messages_and_the_skip_notice_are_rendered() {
    // A Fail whose reason never reaches the reader is a red build with
    // no diagnosis; a skipped case that looks like a completed one is
    // worse, because the checks below the gate never ran at all.
    let results = vec![case_with(
        "gate-stopped",
        Verdict::Fail("gate said no".into()),
        vec![
            check(
                "gate: expected consistent",
                Verdict::Fail("ontology plus data is inconsistent".into()),
            ),
            check(
                "later",
                Verdict::Indeterminate(IndeterminateReason::AxiomLoss(
                    "parse: dropped owl:AllDisjointClasses".into(),
                )),
            ),
        ],
        true,
    )];

    let out = render(&results, &[]);

    assert!(
        out.contains("ontology plus data is inconsistent"),
        "a failing check's message must be rendered verbatim, got: {out}"
    );
    assert!(
        out.contains("indeterminate:") && out.contains("AxiomLoss"),
        "an indeterminate check must render its reason, got: {out}"
    );
    assert!(
        out.contains("remaining checks skipped (see gate)"),
        "a skipped case must say so, or its unrun checks read as passes, got: {out}"
    );
}

// ---------------------------------------------------------------
// `render_json`: the machine format, which must carry the same
// honesty the text one does (ruling 6).
// ---------------------------------------------------------------

/// The four verdicts, one case each, in a fixed order, plus the
/// per-check detail the JSON format is supposed to carry.
fn four_verdict_results() -> Vec<CaseResult> {
    vec![
        case_with(
            "pass-case",
            Verdict::Pass,
            vec![check("pos", Verdict::Pass)],
            false,
        ),
        // The id deliberately does NOT contain the word "unrefuted".
        // It did, and that made the JUnit name assertion below a check
        // that could not fail: the marker test passed on the id alone,
        // so dropping the marker entirely left the test green.
        case_with(
            "negative-only-case",
            Verdict::UnrefutedPass,
            vec![check("neg", Verdict::UnrefutedPass)],
            false,
        ),
        case_with(
            "indet-case",
            Verdict::Indeterminate(IndeterminateReason::Timeout),
            vec![check(
                "slow",
                Verdict::Indeterminate(IndeterminateReason::Timeout),
            )],
            false,
        ),
        case_with(
            "fail-case",
            Verdict::Fail("expected entailed, but no proof was found: x".into()),
            vec![check(
                "pos",
                Verdict::Fail("expected entailed, but no proof was found: x".into()),
            )],
            false,
        ),
    ]
}

fn parse_json(text: &str) -> serde_json::Value {
    serde_json::from_str(text)
        .unwrap_or_else(|e| panic!("render_json must emit valid JSON: {e}\n{text}"))
}

#[test]
fn json_parses_and_names_all_four_verdicts_distinctly() {
    let v = parse_json(&render_json(&four_verdict_results(), &[]));

    let names: Vec<&str> = v["cases"]
        .as_array()
        .expect("cases must be an array")
        .iter()
        .map(|c| c["verdict"].as_str().expect("verdict must be a string"))
        .collect();

    assert_eq!(
        names,
        vec!["pass", "unrefuted_pass", "indeterminate", "fail"],
        "collapsing unrefuted_pass into pass would hand a machine consumer \
         exactly the overstatement this harness exists to prevent"
    );
    assert_eq!(v["summary"]["cases"], 4);
    assert_eq!(v["summary"]["fail"], 1);
    assert_eq!(v["summary"]["unrefuted_pass"], 1);
    assert_eq!(v["summary"]["indeterminate"], 1);
    assert_eq!(
        v["summary"]["unrefuted_checks"], 1,
        "unrefuted CHECKS are counted separately from unrefuted cases"
    );
}

#[test]
fn json_carries_the_failure_message_and_the_indeterminate_kind() {
    let v = parse_json(&render_json(&four_verdict_results(), &[]));
    let cases = v["cases"].as_array().expect("cases must be an array");

    assert_eq!(
        cases[3]["message"], "expected entailed, but no proof was found: x",
        "a Fail's explanation must survive into the machine format"
    );
    assert_eq!(
        cases[2]["indeterminate_kind"], "timeout",
        "a consumer must be able to tell a timeout from axiom loss without \
         pattern-matching on English"
    );
    assert!(
        cases[0]["message"].is_null(),
        "a Pass carries no message, and must not invent one"
    );
}

#[test]
fn json_carries_rests_on_absence_per_check_and_per_case() {
    // Ruling 6. `rests_on_absence` is the flag a check sets when its
    // meaning depends on something NOT being found; dropping it from
    // the machine format restores the overstatement the design exists
    // to prevent.
    let mut cq_check = check("cq questions.rq", Verdict::Pass);
    cq_check.rests_on_absence = true;
    let results = vec![
        case_with("cq-case", Verdict::Pass, vec![cq_check], false),
        case_with(
            "solid-case",
            Verdict::Pass,
            vec![check("pos", Verdict::Pass)],
            false,
        ),
    ];

    let v = parse_json(&render_json(&results, &[]));
    let cases = v["cases"].as_array().expect("cases must be an array");

    assert_eq!(
        cases[0]["checks"][0]["rests_on_absence"], true,
        "the per-check flag must be carried verbatim"
    );
    assert_eq!(
        cases[0]["rests_on_absence"], true,
        "a case holding such a check must roll the flag up"
    );
    assert_eq!(
        cases[1]["rests_on_absence"], false,
        "a case whose checks all rest on something positively found must not \
         claim otherwise, or the flag means nothing"
    );
}

#[test]
fn json_rolls_an_unrefuted_check_into_rests_on_absence() {
    // The per-check flag alone would report `false` for a case whose
    // verdict IS an unrefuted negative, which is the one reading a
    // consumer must never be given: an unrefuted pass is the textbook
    // absence of proof.
    let results = vec![case_with(
        "negative-case",
        Verdict::UnrefutedPass,
        vec![check("neg", Verdict::UnrefutedPass)],
        false,
    )];

    let v = parse_json(&render_json(&results, &[]));

    assert_eq!(
        v["cases"][0]["checks"][0]["rests_on_absence"], false,
        "the raw per-check flag is not set by the entailment path"
    );
    assert_eq!(
        v["cases"][0]["rests_on_absence"], true,
        "but the case-level roll-up must still say the outcome rests on absence"
    );
}

#[test]
fn json_carries_baseline_loss_per_case_and_in_the_summary() {
    // Ruling 6, the other half. The text report already opens with a
    // baseline-loss preamble; a consumer reading only JSON must be able
    // to see the same thing.
    let msg = "conversion: 2 dropped (SubClassOf: unsupported data range x2)";
    let results = vec![
        CaseResult {
            id: "lossy".into(),
            verdict: Verdict::Pass,
            checks: vec![],
            skipped: false,
            baseline_loss: vec![msg.to_string()],
        },
        case_with("clean", Verdict::Pass, vec![], false),
    ];

    let v = parse_json(&render_json(&results, &[]));

    assert_eq!(v["cases"][0]["baseline_loss"][0], msg);
    assert_eq!(
        v["cases"][1]["baseline_loss"].as_array().map(Vec::len),
        Some(0),
        "a case that carried no loss must report none"
    );
    assert_eq!(
        v["summary"]["baseline_loss"][0], msg,
        "the summary must roll baseline loss up, deduplicated"
    );
}

// ---------------------------------------------------------------
// `render_junit`: ruling 7's four-into-three mapping.
// ---------------------------------------------------------------

/// Parse the JUnit output as XML, which is the assertion: a parser
/// rejects the unescaped `<` that a string search would happily match.
fn parse_xml(text: &str) -> roxmltree::Document<'_> {
    roxmltree::Document::parse(text)
        .unwrap_or_else(|e| panic!("render_junit must emit well-formed XML: {e}\n{text}"))
}

/// The `<testcase>` element for `name_starts_with`, or a panic naming
/// what was actually there.
fn testcase<'a, 'd: 'a>(doc: &'a roxmltree::Document<'d>, id: &str) -> roxmltree::Node<'a, 'd> {
    doc.descendants()
        .find(|n| {
            n.has_tag_name("testcase") && n.attribute("name").is_some_and(|v| v.starts_with(id))
        })
        .unwrap_or_else(|| {
            let seen: Vec<&str> = doc
                .descendants()
                .filter(|n| n.has_tag_name("testcase"))
                .filter_map(|n| n.attribute("name"))
                .collect();
            panic!("no testcase named {id}; saw {seen:?}")
        })
}

fn has_child(node: roxmltree::Node, tag: &str) -> bool {
    node.children().any(|c| c.has_tag_name(tag))
}

#[test]
fn junit_maps_all_four_verdicts_as_ruling_7_requires() {
    let xml = render_junit(&four_verdict_results(), &[]);
    let doc = parse_xml(&xml);

    let pass = testcase(&doc, "pass-case");
    assert!(
        !has_child(pass, "failure") && !has_child(pass, "skipped"),
        "a Pass is a plain passing testcase"
    );

    let unrefuted = testcase(&doc, "negative-only-case");
    assert!(
        !has_child(unrefuted, "failure") && !has_child(unrefuted, "skipped"),
        "an UnrefutedPass does not fail the build, matching verdict::exit_code"
    );
    assert_ne!(
        unrefuted.attribute("name"),
        Some("negative-only-case"),
        "JUnit has no fifth state, so the distinction must live in the name: the \
         bare case id is exactly what an UnrefutedPass must NOT render as"
    );
    assert!(
        unrefuted
            .attribute("name")
            .is_some_and(|n| n.contains("unrefuted")),
        "and the name must say which distinction it is carrying, got {:?}",
        unrefuted.attribute("name")
    );
    let sysout = unrefuted
        .children()
        .find(|c| c.has_tag_name("system-out"))
        .expect("an unrefuted pass must carry its caveat in system-out");
    assert!(
        sysout
            .text()
            .is_some_and(|t| t.contains("not a proof of non-entailment")),
        "the system-out line must say what PASS* means, got {:?}",
        sysout.text()
    );

    let indet = testcase(&doc, "indet-case");
    assert!(
        has_child(indet, "skipped"),
        "an Indeterminate is <skipped>: it did not run to a decision"
    );
    assert!(
        !has_child(indet, "failure"),
        "an Indeterminate must NOT turn a consumer's build red on a reasoner timeout"
    );

    let fail = testcase(&doc, "fail-case");
    assert!(has_child(fail, "failure"), "a Fail is <failure>");
}

#[test]
fn junit_counts_only_fails_as_failures() {
    // The attribute a CI dashboard reads. Counting the Indeterminate
    // here would make a reasoner timeout indistinguishable from an
    // ontology regression.
    let doc_text = render_junit(&four_verdict_results(), &[]);
    let doc = parse_xml(&doc_text);
    let suite = doc
        .descendants()
        .find(|n| n.has_tag_name("testsuite"))
        .expect("a testsuite element");

    assert_eq!(suite.attribute("tests"), Some("4"));
    assert_eq!(
        suite.attribute("failures"),
        Some("1"),
        "only the Fail counts as a failure"
    );
    assert_eq!(
        suite.attribute("skipped"),
        Some("1"),
        "only the Indeterminate counts as skipped"
    );
}

#[test]
fn junit_escapes_every_xml_metacharacter_in_a_message() {
    // Verdict messages carry Manchester expressions and full <IRI>
    // forms, so all four of these occur in practice. Unescaped, the
    // `<` alone makes the document unparseable, and a consumer's CI
    // gets a broken report instead of a failing test.
    let msg = r#"expected Feature <= (A & B) > C, but "x" <http://example.org/x> was not"#;
    let results = vec![case_with(
        "meta-case",
        Verdict::Fail(msg.into()),
        vec![],
        false,
    )];

    let xml = render_junit(&results, &[]);
    assert!(
        !xml.contains("<http://example.org/x>"),
        "the raw angle brackets must not reach the document, got: {xml}"
    );

    let doc = parse_xml(&xml);
    let failure = testcase(&doc, "meta-case")
        .children()
        .find(|c| c.has_tag_name("failure"))
        .expect("a Fail must produce a <failure>");

    assert_eq!(
        failure.text(),
        Some(msg),
        "the message must survive escaping and un-escaping unchanged"
    );
    assert_eq!(
        failure.attribute("message"),
        Some(msg),
        "and so must the attribute form, where an unescaped quote would end it early"
    );
}

#[test]
fn junit_carries_baseline_loss_once_and_the_skip_notice_per_case() {
    // Baseline loss is deduplicated to a testsuite-level system-out,
    // mirroring the text report's preamble: on the real suite every
    // case loads the same ontology, so repeating it per testcase
    // produced 66 identical lines and buried the notes that differ.
    // The skip notice, which IS per case, stays per case.
    let loss = "SubClassOf: unsupported data range x2";
    let results = vec![
        CaseResult {
            id: "gate-stopped".into(),
            verdict: Verdict::Pass,
            checks: vec![],
            skipped: true,
            baseline_loss: vec![loss.into()],
        },
        CaseResult {
            id: "other".into(),
            verdict: Verdict::Pass,
            checks: vec![],
            skipped: false,
            baseline_loss: vec![loss.into()],
        },
    ];

    let doc_text = render_junit(&results, &[]);
    let doc = parse_xml(&doc_text);

    assert_eq!(
        doc_text.matches(loss).count(),
        1,
        "the same baseline message from two cases must appear once, got: {doc_text}"
    );
    let suite_note = doc
        .descendants()
        .find(|n| n.has_tag_name("testsuite"))
        .and_then(|s| s.children().find(|c| c.has_tag_name("system-out")))
        .and_then(|n| n.text())
        .unwrap_or_default()
        .to_string();
    assert!(
        suite_note.contains(loss),
        "baseline loss must reach a JUnit reader too, got: {suite_note}"
    );

    let case_note = testcase(&doc, "gate-stopped")
        .children()
        .find(|c| c.has_tag_name("system-out"))
        .and_then(|n| n.text())
        .unwrap_or_default()
        .to_string();
    assert!(
        case_note.contains("remaining checks skipped"),
        "a case that stopped at the gate must say so, got: {case_note}"
    );
    assert!(
        !testcase(&doc, "other")
            .children()
            .any(|c| c.has_tag_name("system-out")),
        "a case with nothing case-specific to say must not carry an empty note"
    );
}

// ---------------------------------------------------------------
// Deferred cases in the machine formats.
//
// Rulings 6 and 7 say a deferred case is "named and counted in every
// format". Until these tests existed that claim was unbacked: every
// render call in this file passed an empty `deferred` slice, and
// `tests/cli.rs` inspected only the text format, so emptying the JSON
// array or dropping the JUnit loop left the whole suite green. A
// promise nothing checks is this project's recurring defect shape.
// ---------------------------------------------------------------

fn one_deferred() -> Vec<DeferredCase> {
    vec![DeferredCase {
        id: "timeinstant-datarange".to_string(),
        path: std::path::PathBuf::from("suites/sulo/restrictions/timeinstant-datarange.yaml"),
        reason: "tagged `oracle-hermit`: checked by the CI differential, not this run".to_string(),
    }]
}

#[test]
fn json_names_and_counts_a_deferred_case() {
    let v = parse_json(&render_json(&four_verdict_results(), &one_deferred()));

    assert_eq!(
        v["summary"]["deferred"], 1,
        "the summary must count deferred cases, or a machine consumer reading only the \
         roll-up cannot tell a run that judged everything from one that skipped a case"
    );

    let deferred = v["deferred"]
        .as_array()
        .expect("the JSON payload must carry a `deferred` array");
    assert_eq!(
        deferred.len(),
        1,
        "the deferred case must be listed, not only counted"
    );
    assert_eq!(deferred[0]["id"], "timeinstant-datarange");
    assert!(
        deferred[0]["reason"]
            .as_str()
            .is_some_and(|r| r.contains("oracle-hermit")),
        "the reason must say WHY it was not run: a deferred case with no reason is \
         indistinguishable from one that was quietly dropped"
    );

    // The cases that DID run are still all there. A `deferred` array
    // built by moving a case out of `cases` would satisfy every
    // assertion above while losing a judged verdict.
    assert_eq!(
        v["cases"].as_array().map(Vec::len),
        Some(four_verdict_results().len()),
        "deferring must not remove a judged case from `cases`"
    );
}

#[test]
fn junit_names_a_deferred_case_and_counts_it_as_a_skip() {
    let xml = render_junit(&four_verdict_results(), &one_deferred());
    let doc = roxmltree::Document::parse(&xml).expect("JUnit output must parse as XML");

    let suite = doc
        .descendants()
        .find(|n| n.has_tag_name("testsuite"))
        .expect("there must be a testsuite element");

    let judged = four_verdict_results().len();
    let indeterminate = 1; // `four_verdict_results` holds exactly one
    assert_eq!(
        suite.attribute("tests"),
        Some((judged + 1).to_string().as_str()),
        "`tests` must include the deferred testcase, or the count contradicts the \
         testcase elements the file actually carries"
    );
    assert_eq!(
        suite.attribute("skipped"),
        Some((indeterminate + 1).to_string().as_str()),
        "`skipped` must include the deferred case"
    );

    let names: Vec<&str> = doc
        .descendants()
        .filter(|n| n.has_tag_name("testcase"))
        .filter_map(|n| n.attribute("name"))
        .collect();
    assert!(
        names
            .iter()
            .any(|n| n.contains("timeinstant-datarange") && n.contains("[deferred]")),
        "the deferred case must appear as a named testcase marked [deferred]: {names:?}"
    );
    assert_eq!(
        names.len(),
        judged + 1,
        "every judged case plus the deferred one must be present: {names:?}"
    );
}

#[test]
fn junit_explains_why_an_indeterminate_is_rendered_as_a_skip() {
    let xml = render_junit(&four_verdict_results(), &[]);
    let doc = roxmltree::Document::parse(&xml).expect("JUnit output must parse as XML");

    let skipped = doc
        .descendants()
        .find(|n| n.has_tag_name("skipped"))
        .expect("the Indeterminate case must render as <skipped>");

    let message = skipped.attribute("message").unwrap_or_default();
    assert!(
        message.contains("exits 3"),
        "ruling 14 promises the report explains that a skip here still exits 3, so a \
         consumer can reconcile a failing job with a report showing no failures: {message}"
    );
    // The body carries it too, because attribute-value normalisation
    // collapses newlines and `<skipped>` has no other surviving copy.
    assert!(
        skipped.text().is_some_and(|t| t.contains("exits 3")),
        "the caveat must survive in the element body, not only the attribute"
    );
}
