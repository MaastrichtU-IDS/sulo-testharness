//! Human-readable output.

use crate::suite::CaseResult;
use crate::verdict::Verdict;

/// Render results, one block per case.
#[must_use]
pub fn render(results: &[CaseResult]) -> String {
    let mut out = String::new();
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
