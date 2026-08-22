//! Human-readable output.

use std::collections::BTreeSet;

use crate::suite::CaseResult;
use crate::verdict::Verdict;

/// Render results, one block per case.
///
/// Opens with a baseline-loss preamble when any case carried known,
/// permanent, pinned-reasoner loss (see `load::Loaded::baseline_loss`
/// and `suite::CaseResult::baseline_loss`): a console warning alone is
/// not a record, and a CI consumer reading only this rendered report
/// still needs to know the run was made over an ontology with axioms
/// the reasoner could not represent, even though that loss never
/// affected any verdict.
#[must_use]
pub fn render(results: &[CaseResult]) -> String {
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

    if unrefuted > 0 {
        out.push_str(&format!(
            "\n{unrefuted} check(s) marked PASS* : a negative expectation the \
             reasoner failed to refute, not a proof of non-entailment.\n"
        ));
    }

    out
}
