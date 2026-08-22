//! Case orchestration.
//!
//! Two rules carry most of the weight:
//!
//! 1. The consistency gate runs first. An inconsistent ontology
//!    entails everything, so running checks against one produces
//!    meaningless passes. Remaining checks are SKIPPED, never passed.
//! 2. Axiom loss downgrades every verdict that rests on "no proof
//!    found". Reasoning over a subset O' of O is monotonic: entailed
//!    by O' implies entailed by O, so a positive Pass and a negative
//!    Fail stay trustworthy. "Not entailed by O'" (equivalently,
//!    "no clash found in O'") says nothing about O, and that answer
//!    underlies FOUR outcomes, not one: a positive-expectation Fail,
//!    a negative-expectation UnrefutedPass, an `expect_inconsistent`
//!    Fail (the gate failed to find the clash it expected), and a
//!    "consistent" verdict from the gate (the gate found no clash at
//!    all). Leaving the last three trusted is how a dropped axiom
//!    becomes a green build.

use std::path::Path;
use std::time::Duration;

use crate::claim::{Claim, parse_fragment};
use crate::load::{load_file, merge};
use crate::manifest::Case;
use crate::oracle::{
    Expectation, check, check_instance_expr, check_satisfiable_expr, check_subsumption_expr,
};
use crate::prefixes::{self, base_mapping, with_overrides};
use crate::verdict::{CheckOutcome, IndeterminateReason, Verdict, aggregate};

/// Name of the consistency-gate check when the case expects
/// inconsistency. Shared between `run_case` (which produces it) and
/// `downgrade_for_loss` (which must recognise it) so the two can
/// never drift apart.
const GATE_EXPECT_INCONSISTENT: &str = "gate: expected inconsistent";
/// Name of the consistency-gate check when the case expects
/// consistency. See `GATE_EXPECT_INCONSISTENT`.
const GATE_EXPECT_CONSISTENT: &str = "gate: expected consistent";

/// The outcome of one case.
pub struct CaseResult {
    pub id: String,
    pub verdict: Verdict,
    pub checks: Vec<CheckOutcome>,
    /// True whenever the consistency gate (or an earlier load
    /// failure) stopped the case before its remaining checks ran.
    /// Never derived as `!gate_failed`: an `expect_inconsistent` case
    /// that PASSES its gate also stops here, and its remaining
    /// checks are genuinely skipped, not passed.
    pub skipped: bool,
    /// Descriptions of loss matching the known, permanent,
    /// pinned-reasoner baseline (see `load::Loaded::baseline_loss`),
    /// accumulated across the ontology and every `imports`/`data`
    /// file this case loaded. Never influences `verdict` (that would
    /// defeat the point of the baseline allowlist), but is carried
    /// here so a report or CI consumer has a machine-visible record
    /// that the run was made over an ontology with axioms the
    /// reasoner could not represent, rather than only a console
    /// warning that scrolls away.
    pub baseline_loss: Vec<String>,
}

/// Downgrade the verdicts that rest on an absence of proof, across
/// ALL FOUR shapes that can carry one: an ordinary positive Fail
/// ("no proof was found"), an ordinary negative UnrefutedPass, the
/// gate's Fail when it expected inconsistency but found none, and the
/// gate's Pass when it found the ontology consistent. A trustworthy
/// positive Pass, a trustworthy negative Fail, and the gate's
/// trustworthy "found inconsistent" outcomes (in either expectation)
/// are left untouched: each rests on a clash or entailment actually
/// found, which loss (a strict subset of the intended axioms) cannot
/// manufacture out of nothing.
pub fn downgrade_for_loss(outcomes: &mut Vec<CheckOutcome>, loss: &[String]) {
    if loss.is_empty() {
        return;
    }
    let reason = loss.join("; ");

    for out in outcomes.iter_mut() {
        let untrusted = match (out.name.as_str(), &out.verdict) {
            // Rests on "no proof found".
            (_, Verdict::UnrefutedPass) => true,
            // A positive expectation that found no proof.
            (_, Verdict::Fail(msg)) if msg.contains("no proof was found") => true,
            // Gate expected inconsistency, found none: rests on
            // absence of a clash, exactly analogous to the two cases
            // above.
            (GATE_EXPECT_INCONSISTENT, Verdict::Fail(_)) => true,
            // Gate found the ontology consistent: also rests on
            // absence of a clash over a possibly-weakened ontology.
            (GATE_EXPECT_CONSISTENT, Verdict::Pass) => true,
            _ => false,
        };
        if untrusted {
            out.verdict = Verdict::Indeterminate(IndeterminateReason::AxiomLoss(reason.clone()));
        }
    }
}

/// Run one case end to end.
pub fn run_case(case: &Case, default_ontology: &Path) -> CaseResult {
    let mut checks = Vec::new();

    // Resolve and load.
    let onto_path = case
        .ontology
        .as_ref()
        .map(|p| case.base_dir.join(p))
        .unwrap_or_else(|| default_ontology.to_path_buf());

    let loaded = match load_file(&onto_path) {
        Ok(l) => l,
        Err(e) => {
            return CaseResult {
                id: case.id.clone(),
                verdict: Verdict::Indeterminate(IndeterminateReason::OracleError(e.to_string())),
                checks,
                skipped: true,
                baseline_loss: Vec::new(),
            };
        }
    };

    let mut onto = loaded.ontology;
    let mut loss = loaded.loss;
    let mut baseline_loss = loaded.baseline_loss;

    for extra in case.imports.iter().chain(case.data.iter()) {
        match load_file(&case.base_dir.join(extra)) {
            Ok(l) => {
                loss.extend(l.loss);
                baseline_loss.extend(l.baseline_loss);
                merge(&mut onto, l.ontology);
            }
            Err(e) => {
                return CaseResult {
                    id: case.id.clone(),
                    verdict: Verdict::Indeterminate(IndeterminateReason::OracleError(
                        e.to_string(),
                    )),
                    checks,
                    skipped: true,
                    baseline_loss,
                };
            }
        }
    }

    let pm = with_overrides(&base_mapping(), &case.prefixes);

    // The case's own time budget, per check. A `timeout_ms` of 0
    // means "expire immediately" (a deterministic Timeout on every
    // check it governs), not "no limit": that matches the zero-
    // deadline seam `holds_with_deadline` already uses elsewhere in
    // this crate to force a Timeout without relying on a real
    // reasoner call being slow. See `manifest::Case::timeout_ms`.
    let deadline = Duration::from_millis(case.timeout_ms);

    // Gate: consistency before anything else. An inconsistent
    // ontology entails everything, so any check run against one would
    // be a meaningless pass.
    let consistent = match owl_dl_reasoner::is_consistent(&onto) {
        Ok(c) => c,
        Err(e) => {
            return CaseResult {
                id: case.id.clone(),
                verdict: Verdict::Indeterminate(IndeterminateReason::OracleError(e.to_string())),
                checks,
                skipped: true,
                baseline_loss,
            };
        }
    };

    let gate = match (case.expect_inconsistent, consistent) {
        (true, false) => CheckOutcome {
            name: GATE_EXPECT_INCONSISTENT.into(),
            verdict: Verdict::Pass,
        },
        (true, true) => CheckOutcome {
            name: GATE_EXPECT_INCONSISTENT.into(),
            // "Consistent" is the direction soundness does not vouch
            // for, and is_consistent exposes no incomplete flag.
            verdict: Verdict::Fail(
                "expected inconsistent, but the reasoner found it consistent; \
                 an axiom may have stopped biting. The CI differential settles it."
                    .into(),
            ),
        },
        (false, false) => CheckOutcome {
            name: GATE_EXPECT_CONSISTENT.into(),
            verdict: Verdict::Fail(
                "ontology plus data is inconsistent, so every entailment check \
                 below would pass vacuously. Remaining checks skipped."
                    .into(),
            ),
        },
        (false, true) => CheckOutcome {
            name: GATE_EXPECT_CONSISTENT.into(),
            verdict: Verdict::Pass,
        },
    };

    let gate_stops_here = matches!(gate.verdict, Verdict::Fail(_)) || case.expect_inconsistent;
    checks.push(gate);

    if gate_stops_here {
        downgrade_for_loss(&mut checks, &loss);
        let verdict = aggregate(&checks);
        return CaseResult {
            id: case.id.clone(),
            verdict,
            checks,
            skipped: gate_stops_here,
            baseline_loss,
        };
    }

    // Positive and negative Turtle-fragment claims.
    for (fragment, expect) in [
        (case.entails.as_ref(), Expectation::Entailed),
        (case.not_entails.as_ref(), Expectation::NotEntailed),
    ] {
        if let Some(text) = fragment {
            match parse_fragment(text, &pm) {
                Ok(claims) => {
                    for claim in &claims {
                        checks.push(check(&onto, claim, expect, deadline));
                    }
                }
                Err(e) => checks.push(CheckOutcome {
                    name: "fragment parse".into(),
                    verdict: Verdict::Indeterminate(IndeterminateReason::OracleError(
                        e.to_string(),
                    )),
                }),
            }
        }
    }

    // Class-expression claims.
    for s in &case.entails_manchester {
        checks.push(check_subsumption_expr(
            &onto,
            &s.sub_expr,
            &s.sup_expr,
            Expectation::Entailed,
            &pm,
            deadline,
        ));
    }
    for s in &case.not_entails_manchester {
        checks.push(check_subsumption_expr(
            &onto,
            &s.sub_expr,
            &s.sup_expr,
            Expectation::NotEntailed,
            &pm,
            deadline,
        ));
    }
    for i in &case.instance_of_expr {
        checks.push(check_instance_expr(
            &onto,
            &i.individual,
            &i.expr,
            Expectation::Entailed,
            &pm,
            deadline,
        ));
    }
    for e in &case.satisfiable_expr {
        checks.push(check_satisfiable_expr(
            &onto,
            e,
            Expectation::Entailed,
            &pm,
            deadline,
        ));
    }
    for class in &case.unsatisfiable {
        match prefixes::expand(&pm, class) {
            Ok(iri) => {
                let claim = Claim::Unsatisfiable { class: iri };
                checks.push(check(&onto, &claim, Expectation::Entailed, deadline));
            }
            // Do not silently fall back to the raw, unexpanded token:
            // that would ask the reasoner about a class IRI the
            // author never meant, and either report a confusing Fail
            // or (worse) a silent UnrefutedPass. Surface the prefix
            // mistake instead, same as a bad fragment parse above.
            Err(e) => checks.push(CheckOutcome {
                name: format!("unsatisfiable: {class}"),
                verdict: Verdict::Indeterminate(IndeterminateReason::OracleError(e.to_string())),
            }),
        }
    }

    downgrade_for_loss(&mut checks, &loss);
    let verdict = aggregate(&checks);

    CaseResult {
        id: case.id.clone(),
        verdict,
        checks,
        skipped: false,
        baseline_loss,
    }
}
