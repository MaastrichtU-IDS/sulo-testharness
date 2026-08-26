//! Human-readable output.

use std::collections::BTreeSet;

use serde_json::json;

use crate::suite::{CaseResult, DeferredCase};
use crate::verdict::{IndeterminateReason, Verdict};

/// Render results, one block per case.
///
/// Opens with a baseline-loss preamble when any case carried known,
/// permanent, pinned-reasoner loss (see `load::Loaded::baseline_loss`
/// and `suite::CaseResult::baseline_loss`): a console warning alone is
/// not a record, and a CI consumer reading only this rendered report
/// still needs to know the run was made over an ontology with axioms
/// the reasoner could not represent, even though that loss never
/// affected any verdict.
///
/// Closes with the cases that were selected and deliberately NOT run
/// (`suite::DeferredCase`). They are named and counted here rather
/// than dropped, because a suppressed case that nothing prints is a
/// silenced failure.
#[must_use]
pub fn render(results: &[CaseResult], deferred: &[DeferredCase]) -> String {
    let mut out = String::new();

    let baseline: BTreeSet<&str> = results
        .iter()
        .flat_map(|r| r.baseline_loss.iter().map(String::as_str))
        .collect();
    if !baseline.is_empty() {
        out.push_str(
            "NOTE: known baseline loss (pinned-reasoner limitation, not an \
             ontology defect; did not affect any verdict below):\n",
        );
        for b in &baseline {
            out.push_str(&format!("      {b}\n"));
        }
        out.push('\n');
    }

    let mut unrefuted = 0usize;

    for r in results {
        let tag = match &r.verdict {
            Verdict::Pass => "PASS",
            Verdict::UnrefutedPass => "PASS*",
            Verdict::Indeterminate(_) => "INDET",
            Verdict::Fail(_) => "FAIL",
        };
        out.push_str(&format!("{tag:<6} {}\n", r.id));

        for c in &r.checks {
            match &c.verdict {
                Verdict::UnrefutedPass => unrefuted += 1,
                Verdict::Fail(msg) => out.push_str(&format!("         {msg}\n")),
                Verdict::Indeterminate(reason) => {
                    out.push_str(&format!("         indeterminate: {reason:?}\n"));
                }
                Verdict::Pass => {}
            }
        }

        if r.skipped {
            out.push_str("         remaining checks skipped (see gate)\n");
        }
    }

    // Named and counted, never silently dropped. Printed even though
    // these cases set no verdict: a reader must be able to see that
    // the run covered 65 of 66 cases without going and grepping the
    // suite for the tag.
    if !deferred.is_empty() {
        out.push_str(&format!(
            "\n{} case(s) DEFERRED, selected but not run, and excluded from the \
             exit code:\n",
            deferred.len()
        ));
        for d in deferred {
            out.push_str(&format!("DEFER  {} ({})\n", d.id, d.path.display()));
            out.push_str(&format!("         {}\n", d.reason));
        }
    }

    if unrefuted > 0 {
        out.push_str(&format!(
            "\n{unrefuted} check(s) marked PASS* : a negative expectation the \
             reasoner failed to refute, not a proof of non-entailment.\n"
        ));
    }

    out
}

/// The wire name of a verdict, shared by both machine formats.
///
/// `UnrefutedPass` gets its own name rather than collapsing into
/// `pass`. A consumer that cannot tell the two apart has been handed
/// exactly the overstatement (`Verdict`'s four-way split, and this
/// whole harness) exists to prevent.
fn verdict_name(v: &Verdict) -> &'static str {
    match v {
        Verdict::Pass => "pass",
        Verdict::UnrefutedPass => "unrefuted_pass",
        Verdict::Indeterminate(_) => "indeterminate",
        Verdict::Fail(_) => "fail",
    }
}

/// The explanation a verdict carries, if any. `None` for the two
/// passing verdicts, which carry no message.
fn verdict_message(v: &Verdict) -> Option<String> {
    match v {
        Verdict::Pass | Verdict::UnrefutedPass => None,
        Verdict::Fail(msg) => Some(msg.clone()),
        Verdict::Indeterminate(reason) => Some(match reason {
            IndeterminateReason::Timeout => "the reasoner exceeded the case's time budget".into(),
            IndeterminateReason::AxiomLoss(d) => format!("axiom loss: {d}"),
            IndeterminateReason::OracleError(d) => format!("oracle error: {d}"),
        }),
    }
}

/// Which kind of `Indeterminate` this is, as a stable machine token,
/// or `None` for any other verdict. A consumer needs to distinguish a
/// reasoner timeout (retry, raise `timeout_ms`) from axiom loss (the
/// ontology carries something the reasoner cannot represent) without
/// pattern-matching on English.
fn indeterminate_kind(v: &Verdict) -> Option<&'static str> {
    match v {
        Verdict::Indeterminate(IndeterminateReason::Timeout) => Some("timeout"),
        Verdict::Indeterminate(IndeterminateReason::AxiomLoss(_)) => Some("axiom_loss"),
        Verdict::Indeterminate(IndeterminateReason::OracleError(_)) => Some("oracle_error"),
        _ => None,
    }
}

/// Render results as JSON.
///
/// Two fields exist here purely so the machine format is as honest as
/// the text one:
///
/// * `baseline_loss`, per case and rolled up in the summary: the run
///   was made over an ontology carrying axioms the pinned reasoner
///   could not represent.
/// * `rests_on_absence`, per check verbatim from
///   `CheckOutcome::rests_on_absence`, and per case as a ROLL-UP with
///   a deliberately wider definition: a case rests on absence if any
///   of its checks sets the flag OR came out `UnrefutedPass`. The
///   per-check flag alone would report `false` for a case whose whole
///   verdict is an unrefuted negative, which is precisely the reading
///   a consumer must not be given. The two are not the same predicate,
///   so they are documented rather than quietly conflated.
///
/// `report` decides no verdicts (spec section 6): every value below is
/// read off the results, and no overall verdict is computed here. The
/// caller aggregates through `verdict::aggregate` and exits on
/// `verdict::exit_code`.
#[must_use]
pub fn render_json(results: &[CaseResult], deferred: &[DeferredCase]) -> String {
    let mut cases = Vec::with_capacity(results.len());
    let mut unrefuted_checks = 0usize;

    for r in results {
        let mut checks = Vec::with_capacity(r.checks.len());
        for c in &r.checks {
            if c.verdict == Verdict::UnrefutedPass {
                unrefuted_checks += 1;
            }
            checks.push(json!({
                "name": c.name,
                "verdict": verdict_name(&c.verdict),
                "message": verdict_message(&c.verdict),
                "indeterminate_kind": indeterminate_kind(&c.verdict),
                "rests_on_absence": c.rests_on_absence,
            }));
        }

        let rests_on_absence = r
            .checks
            .iter()
            .any(|c| c.rests_on_absence || c.verdict == Verdict::UnrefutedPass);

        cases.push(json!({
            "id": r.id,
            "verdict": verdict_name(&r.verdict),
            "message": verdict_message(&r.verdict),
            "indeterminate_kind": indeterminate_kind(&r.verdict),
            "skipped": r.skipped,
            "rests_on_absence": rests_on_absence,
            "baseline_loss": r.baseline_loss,
            "checks": checks,
        }));
    }

    let baseline: BTreeSet<&str> = results
        .iter()
        .flat_map(|r| r.baseline_loss.iter().map(String::as_str))
        .collect();
    let count = |name: &str| {
        results
            .iter()
            .filter(|r| verdict_name(&r.verdict) == name)
            .count()
    };

    // Deferred cases are their own array, not entries in `cases` with
    // a flag: a machine consumer summing `summary.pass + fail + ...`
    // must not be able to mistake one for a case that was judged.
    let deferred_json: Vec<_> = deferred
        .iter()
        .map(|d| {
            json!({
                "id": d.id,
                "path": d.path.to_string_lossy(),
                "reason": d.reason,
            })
        })
        .collect();

    let payload = json!({
        "summary": {
            "cases": results.len(),
            "pass": count("pass"),
            "unrefuted_pass": count("unrefuted_pass"),
            "indeterminate": count("indeterminate"),
            "fail": count("fail"),
            "deferred": deferred.len(),
            "unrefuted_checks": unrefuted_checks,
            "baseline_loss": baseline.iter().collect::<Vec<_>>(),
        },
        "cases": cases,
        "deferred": deferred_json,
    });

    // `to_string_pretty` only fails on a non-string map key or a
    // non-finite float, neither of which this tree can contain.
    serde_json::to_string_pretty(&payload).unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
}

/// Escape text for inclusion in XML character data or a
/// double-quoted attribute value.
///
/// Not optional and not defensive: verdict messages carry Manchester
/// expressions (`Feature and (Capability or ...)`), full `<IRI>`
/// forms, and quoted literals, so `<`, `>`, `&` and `"` all occur in
/// practice. `&` must be replaced first, or the ampersands introduced
/// by the later replacements would be escaped a second time.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The `<system-out>` line that carries an unrefuted pass's caveat.
const UNREFUTED_NOTE: &str = "PASS*: a negative expectation the reasoner failed to refute, \
                              not a proof of non-entailment.";

/// Render results as a JUnit XML report.
///
/// The verdict mapping, and why JUnit's three states have to carry
/// four:
///
/// | Verdict | JUnit |
/// | --- | --- |
/// | `Pass` | a plain `<testcase>` |
/// | `UnrefutedPass` | a plain `<testcase>`, marked in `name` and in `<system-out>` |
/// | `Indeterminate` | `<skipped>` |
/// | `Fail` | `<failure>` |
///
/// A deferred case (`suite::DeferredCase`) is not a verdict at all,
/// and is emitted as a `<skipped>` testcase marked `[deferred]` in the
/// name. It counts toward `tests` and toward `skipped`.
///
/// `Indeterminate` is deliberately NOT a failure: a reasoner timeout
/// or an axiom the parser dropped must not turn a consumer's build red
/// as though SULO had regressed. It is equally deliberately not a
/// plain pass, because nothing was verified. `<skipped>` is the only
/// JUnit state that says "this did not run" rather than "this was
/// fine".
///
/// `UnrefutedPass` has no JUnit state at all, so its distinction is
/// carried in the testcase name and in a `<system-out>` line rather
/// than dropped. It does not fail the build, matching
/// `verdict::exit_code`.
#[must_use]
pub fn render_junit(results: &[CaseResult], deferred: &[DeferredCase]) -> String {
    let failures = results
        .iter()
        .filter(|r| matches!(r.verdict, Verdict::Fail(_)))
        .count();
    // Deferred cases count as skips here, and toward `tests`, because
    // they are emitted below as `<testcase><skipped/>`. A count that
    // did not include them would leave a CI report claiming fewer
    // tests than the testcase elements it carries.
    let skipped = results
        .iter()
        .filter(|r| matches!(r.verdict, Verdict::Indeterminate(_)))
        .count()
        + deferred.len();
    let total = results.len() + deferred.len();

    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str(&format!(
        "<testsuites tests=\"{total}\" failures=\"{failures}\" skipped=\"{skipped}\" errors=\"0\">\n"
    ));
    out.push_str(&format!(
        "  <testsuite name=\"sulo-testharness\" tests=\"{total}\" failures=\"{failures}\" \
         skipped=\"{skipped}\" errors=\"0\">\n"
    ));

    // Baseline loss goes here, DEDUPLICATED, exactly as `render` puts
    // it in a preamble rather than on every line. On the real suite
    // all 66 cases load the same ontology and so carry the same single
    // message; repeating it per testcase produced 66 identical lines
    // and buried the per-case notes that actually differ. Per-case
    // attribution is not lost to a machine consumer: `render_json`
    // keeps `baseline_loss` on every case (ruling 6).
    let baseline: BTreeSet<&str> = results
        .iter()
        .flat_map(|r| r.baseline_loss.iter().map(String::as_str))
        .collect();
    if !baseline.is_empty() {
        let mut note = String::from(
            "known baseline loss (pinned-reasoner limitation, not an ontology defect, \
             did not affect any verdict below):",
        );
        for b in &baseline {
            note.push_str("\n  ");
            note.push_str(b);
        }
        out.push_str(&format!(
            "    <system-out>{}</system-out>\n",
            xml_escape(&note)
        ));
    }

    for r in results {
        let name = match r.verdict {
            Verdict::UnrefutedPass => format!("{} [unrefuted]", r.id),
            _ => r.id.clone(),
        };
        out.push_str(&format!(
            "    <testcase classname=\"sulo-testharness\" name=\"{}\">\n",
            xml_escape(&name)
        ));

        match &r.verdict {
            Verdict::Fail(msg) => {
                let e = xml_escape(msg);
                out.push_str(&format!(
                    "      <failure message=\"{e}\" type=\"fail\">{e}</failure>\n"
                ));
            }
            Verdict::Indeterminate(_) => {
                // `verdict_message` rather than a local string, so the
                // JSON and JUnit explanations cannot drift apart.
                let e = xml_escape(&verdict_message(&r.verdict).unwrap_or_default());
                out.push_str(&format!("      <skipped message=\"{e}\"/>\n"));
            }
            Verdict::Pass | Verdict::UnrefutedPass => {}
        }

        let mut notes: Vec<String> = Vec::new();
        if r.verdict == Verdict::UnrefutedPass {
            notes.push(UNREFUTED_NOTE.to_string());
        }
        if r.skipped {
            notes.push("remaining checks skipped (see gate)".into());
        }
        if !notes.is_empty() {
            out.push_str(&format!(
                "      <system-out>{}</system-out>\n",
                xml_escape(&notes.join("\n"))
            ));
        }

        out.push_str("    </testcase>\n");
    }

    // Deferred cases are emitted as skips, marked in the name, so a
    // consumer reading only the JUnit report still sees every case
    // that was selected and can tell which ones were not judged.
    // `<skipped>` is the same state `Indeterminate` maps to, and for
    // the same reason: it says "this did not run", not "this was
    // fine".
    for d in deferred {
        out.push_str(&format!(
            "    <testcase classname=\"sulo-testharness\" name=\"{}\">\n",
            xml_escape(&format!("{} [deferred]", d.id))
        ));
        out.push_str(&format!(
            "      <skipped message=\"{}\"/>\n",
            xml_escape(&d.reason)
        ));
        out.push_str("    </testcase>\n");
    }

    out.push_str("  </testsuite>\n</testsuites>\n");
    out
}
