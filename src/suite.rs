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
//!    becomes a green build. The competency-question path adds
//!    further absence-resting outcomes that neither the verdict text
//!    nor the check name can identify, so those declare themselves
//!    via `CheckOutcome::rests_on_absence` instead; see
//!    `downgrade_for_loss` and `cq::check_cq`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::claim::{Claim, parse_fragment};
use crate::cq::check_cq;
use crate::load::{load_file, merge};
use crate::manifest::{Case, load_case};
use crate::materialize::materialize;
use crate::oracle::{
    Expectation, NO_PROOF_MARKER, check, check_instance_expr, check_satisfiable_expr,
    check_subsumption_expr,
};
use crate::prefixes::{self, base_mapping, with_overrides};
use crate::verdict::{CheckOutcome, IndeterminateReason, Verdict, aggregate};

/// Name of the consistency-gate check when the case expects
/// inconsistency. Shared between `run_case` (which produces it),
/// `downgrade_for_loss` (which must recognise it) and
/// `differential::questions` (which pairs its own gate question with
/// the outcome recorded under this name) so the three can never drift
/// apart. Public for that third reader: a differential that looked up
/// a name nobody produces would find no rustdl answer for every case
/// and report Indeterminate for the whole suite.
pub const GATE_EXPECT_INCONSISTENT: &str = "gate: expected inconsistent";
/// Name of the consistency-gate check when the case expects
/// consistency. See `GATE_EXPECT_INCONSISTENT`.
pub const GATE_EXPECT_CONSISTENT: &str = "gate: expected consistent";

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

/// Downgrade the verdicts that rest on an absence of proof.
///
/// FOUR of the shapes are recognised structurally, from the verdict
/// text or the gate's own check name: an ordinary positive Fail ("no
/// proof was found"), an ordinary negative UnrefutedPass, the gate's
/// Fail when it expected inconsistency but found none, and the gate's
/// Pass when it found the ontology consistent. A FIFTH route exists
/// for check kinds those two structural signals cannot see, and
/// declares itself through `CheckOutcome::rests_on_absence`; the
/// competency-question path is the only current user (see
/// `cq::check_cq`). A trustworthy positive Pass, a trustworthy
/// negative Fail, and the gate's trustworthy "found inconsistent"
/// outcomes (in either expectation) are left untouched: each rests on
/// a clash or entailment actually found, which loss (a strict subset
/// of the intended axioms) cannot manufacture out of nothing.
///
/// Reachability, stated honestly: SULO's only current loss is the two
/// data-range axioms `load.rs` routes to `baseline_loss`, which is
/// never passed to this function, so no case in this repository
/// reaches any branch below today. The `rests_on_absence` route in
/// particular is a LATENT hole, closed against a future
/// `data:`/`imports:` file carrying an axiom horned-owl cannot
/// convert, not a bug observable on the suite as it stands.
pub fn downgrade_for_loss(outcomes: &mut [CheckOutcome], loss: &[String]) {
    if loss.is_empty() {
        return;
    }
    let reason = loss.join("; ");

    for out in outcomes.iter_mut() {
        // The self-declared route, checked first because it is the
        // one the two structural signals below cannot express.
        if out.rests_on_absence {
            out.verdict = Verdict::Indeterminate(IndeterminateReason::AxiomLoss(reason.clone()));
            continue;
        }
        let untrusted = match (out.name.as_str(), &out.verdict) {
            // Rests on "no proof found".
            (_, Verdict::UnrefutedPass) => true,
            // A positive expectation that found no proof. Matched
            // against `oracle::NO_PROOF_MARKER`, the same constant
            // the message is built from, so editing the wording can
            // never silently switch this downgrade off.
            (_, Verdict::Fail(msg)) if msg.contains(NO_PROOF_MARKER) => true,
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
///
/// # The consistency gate is UNBOUNDED
///
/// Stated here rather than left to be rediscovered: the gate below
/// calls `owl_dl_reasoner::is_consistent`, which at the pinned
/// v0.4.22 has NO deadline-bearing variant (`is_consistent_with_stats`
/// takes none either). So the gate cannot honour the case's
/// `timeout_ms`, has no `Indeterminate(Timeout)` route, and a
/// pathological ontology or data file blocks the whole suite. Every
/// other reasoner call this crate makes is bounded (see `oracle`'s
/// module doc); this one is the exception.
///
/// Expressing the gate as a bounded probe on `owl:Thing`
/// satisfiability was tried and rejected. It agrees with
/// `is_consistent` on every fixture in this repository, including the
/// purely ABox-driven inconsistencies, but it is not a
/// proven-equivalent oracle: `is_class_satisfiable_with_timeout` skips
/// the two ABox pre-checks `is_consistent` runs
/// (`abox_saturation_inconsistent` and `abox_verdict`) and
/// short-circuits to `Ok(Some(true))` on a pure-EL ontology. Trading
/// an unbounded gate for a gate that might MISS an inconsistency is
/// strictly the worse deal: a missed inconsistency makes every check
/// below pass vacuously, which is the exact failure this gate exists
/// to prevent. Revisit when rustdl exposes a deadline on
/// `is_consistent`, not before.
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
    // means "expire immediately", not "no limit". It forces a Timeout
    // only on work that actually consults the deadline, which is not
    // every check: see `manifest::Case::timeout_ms`, where the
    // measurement and the earlier overstatement are recorded.
    let deadline = Duration::from_millis(case.timeout_ms);

    // Gate: consistency before anything else. An inconsistent
    // ontology entails everything, so any check run against one would
    // be a meaningless pass. This call is unbounded and ignores
    // `deadline`: see this function's doc comment for why, and for why
    // the obvious bounded substitute was rejected rather than
    // overlooked.
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
            rests_on_absence: false,
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
            rests_on_absence: false,
        },
        (false, false) => CheckOutcome {
            name: GATE_EXPECT_CONSISTENT.into(),
            verdict: Verdict::Fail(
                "ontology plus data is inconsistent, so every entailment check \
                 below would pass vacuously. Remaining checks skipped."
                    .into(),
            ),
            rests_on_absence: false,
        },
        (false, true) => CheckOutcome {
            name: GATE_EXPECT_CONSISTENT.into(),
            verdict: Verdict::Pass,
            rests_on_absence: false,
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
                // A fragment that parses to ZERO claims is a case that
                // asserts nothing while reporting a confident green:
                // `entails: |` holding only whitespace or a `#`
                // comment is valid Turtle producing no triples, so the
                // loop below would push no checks and `aggregate`
                // would return Pass over an empty set. Surface it as
                // the configuration error it is. `manifest::load_case`
                // catches the coarser form of the same mistake (no
                // assertion field at all).
                Ok(claims) if claims.is_empty() => checks.push(CheckOutcome {
                    name: "empty fragment".into(),
                    verdict: Verdict::Indeterminate(IndeterminateReason::OracleError(format!(
                        "the {} fragment parsed to zero claims, so this case would \
                         assert nothing and still report a pass; it is empty or \
                         contains only comments",
                        match expect {
                            Expectation::Entailed => "entails",
                            Expectation::NotEntailed => "not_entails",
                        }
                    ))),
                    rests_on_absence: false,
                }),
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
                    rests_on_absence: false,
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
        // `individual:` is a CURIE or a full `<IRI>` like every other
        // entity-naming field (spec 7.2), resolved through the same
        // prefix map before it ever reaches the reasoner. Do not
        // silently fall back to the raw, unresolved token: that would
        // ask the reasoner about an individual the author never
        // meant, same reasoning as the `unsatisfiable` loop below.
        match prefixes::expand(&pm, &i.individual) {
            Ok(individual) => checks.push(check_instance_expr(
                &onto,
                &individual,
                &i.expr,
                Expectation::Entailed,
                &pm,
                deadline,
            )),
            Err(e) => checks.push(CheckOutcome {
                name: format!("instance_of_expr individual: {}", i.individual),
                verdict: Verdict::Indeterminate(IndeterminateReason::OracleError(e.to_string())),
                rests_on_absence: false,
            }),
        }
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
                rests_on_absence: false,
            }),
        }
    }

    // Competency questions. Materialised ONCE per case, not once per
    // question: `materialize` costs roughly 16ms on real SULO, and
    // paying that per CQ would multiply it by the question count for
    // no benefit. Only paid at all when the case actually has a `cq:`
    // block. The gate above already stopped this function before here
    // whenever the ontology was inconsistent, so every CQ below runs
    // against a store built from a genuinely consistent ontology.
    //
    // A `MaterializeError` means the store was never built, so none
    // of the case's competency questions were actually asked: that is
    // an `Indeterminate`, carrying the error text, for every `cq`
    // entry, never a `Fail`. Reporting a build failure as a Fail would
    // look exactly like an ontology regression.
    if !case.cq.is_empty() {
        match materialize(&onto, deadline) {
            Ok(store) => {
                for spec in &case.cq {
                    checks.push(check_cq(&store, spec, &case.base_dir, &pm));
                }
            }
            Err(e) => {
                let msg = e.to_string();
                for spec in &case.cq {
                    checks.push(CheckOutcome {
                        name: format!("cq {}", spec.query.display()),
                        verdict: Verdict::Indeterminate(IndeterminateReason::OracleError(
                            msg.clone(),
                        )),
                        rests_on_absence: false,
                    });
                }
            }
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

/// Directory names holding fixtures rather than cases.
///
/// A `.yaml` under one of these is data for a case, never a case
/// itself, so `discover` does not descend into them. Named once here
/// so the walk and the error message that mentions them cannot drift
/// apart.
pub const FIXTURE_DIRS: [&str; 2] = ["data", "queries"];

/// Everything that stops a run before any case is judged.
///
/// Every variant is a configuration error (exit code 2 per spec 5.4),
/// never a verdict about the ontology: a suite that could not be read
/// has told us nothing about SULO, and reporting it as a failing case
/// would look exactly like an ontology regression.
#[derive(Debug, thiserror::Error)]
pub enum SuiteError {
    #[error(
        "suite root {path} does not exist: pass --suite the path to a directory \
         of case manifests"
    )]
    RootMissing { path: PathBuf },
    #[error(
        "suite root {path} is not a directory: pass --suite a directory of case \
         manifests, not a single file"
    )]
    RootNotDirectory { path: PathBuf },
    #[error("cannot read {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error(
        "suite root {path} holds no case manifests, so this run would check \
         nothing and still report a pass: add a *.yaml case, or point --suite at \
         the directory that holds them. Note that a *.yaml inside a data/ or \
         queries/ directory is a fixture and is deliberately not discovered."
    )]
    NoCases { path: PathBuf },
    #[error(
        "{path} has the extension .yml, but case manifests are discovered as \
         *.yaml only, so this file would be read by nobody and reported by \
         nothing. Rename it to .yaml. (A *.yml inside a data/ or queries/ \
         directory is a fixture, not a case, and is not refused.)"
    )]
    StrayYml { path: PathBuf },
    #[error(
        "--deferred only was requested, but none of the {selected} selected \
         case(s) under {path} carries the `{tag}` tag, so this run would check \
         nothing and still report a pass"
    )]
    NoDeferredCases {
        path: PathBuf,
        selected: usize,
        tag: &'static str,
    },
    #[error(
        "every one of the {selected} selected case(s) under {path} carries the \
         `{tag}` tag and is deferred by default, so this run would check nothing \
         and still report a pass. Pass --deferred include or --deferred only to \
         execute them."
    )]
    AllCasesDeferred {
        path: PathBuf,
        selected: usize,
        tag: &'static str,
    },
}

/// Find every case manifest under `root`, sorted.
///
/// Recursive, `*.yaml` only, skipping any directory named in
/// `FIXTURE_DIRS`. Sorted so that the report, the JSON payload and the
/// JUnit file come out in the same order on every machine, which is
/// what makes two runs diffable.
///
/// # Zero cases is an error, not a pass
///
/// A suite root that silently matches nothing is a check that cannot
/// fail: the run would report a confident green having asked the
/// reasoner nothing at all. It is refused here (`NoCases`) rather than
/// aggregated into a Pass over an empty set.
///
/// # The fixture-directory skip is relative to `root`
///
/// Only components BELOW `root` are inspected. A checkout that happens
/// to live under a directory called `data` (say
/// `/home/me/data/sulo-testharness`) would otherwise discover nothing
/// anywhere, which is the same silent-empty-suite failure wearing a
/// different hat. Pointing `--suite` directly at a fixture directory
/// therefore does discover the fixtures, and each one then fails to
/// load as a manifest, which is exit 2 and an honest message rather
/// than a green run.
pub fn discover(root: &Path) -> Result<Vec<PathBuf>, SuiteError> {
    if !root.exists() {
        return Err(SuiteError::RootMissing {
            path: root.to_path_buf(),
        });
    }
    if !root.is_dir() {
        return Err(SuiteError::RootNotDirectory {
            path: root.to_path_buf(),
        });
    }

    let mut found = Vec::new();
    walk(root, &mut found)?;

    if found.is_empty() {
        return Err(SuiteError::NoCases {
            path: root.to_path_buf(),
        });
    }

    found.sort();
    Ok(found)
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), SuiteError> {
    let entries = std::fs::read_dir(dir).map_err(|source| SuiteError::Io {
        path: dir.to_path_buf(),
        source,
    })?;

    for entry in entries {
        let entry = entry.map_err(|source| SuiteError::Io {
            path: dir.to_path_buf(),
            source,
        })?;
        let path = entry.path();

        // `path.is_dir()` rather than `entry.file_type()`: the former
        // follows symlinks, so a suite assembled out of symlinked
        // group directories still walks.
        if path.is_dir() {
            if FIXTURE_DIRS
                .iter()
                .any(|d| entry.file_name().as_os_str() == *d)
            {
                continue;
            }
            walk(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "yaml") {
            out.push(path);
        } else if path.extension().is_some_and(|e| e == "yml") {
            // Refused, not skipped. Discovery matches `*.yaml`, so a
            // `.yml` case would be passed over in silence: an author
            // would see a green run over 65 cases and have no way to
            // learn that their 66th was never read. That is this
            // project's recurring defect shape (a check that cannot
            // fail) arriving through a filename. One convention,
            // enforced loudly. Fixture directories never reach here,
            // because the branch above skips them without descending,
            // so a `data/*.yml` query fixture stays legal.
            return Err(SuiteError::StrayYml { path });
        }
    }

    Ok(())
}

/// The tag marking a case whose oracle of record is HermiT, not the
/// pinned rustdl build.
///
/// Spec line 746: a case asserting something rustdl provably cannot
/// enforce (today, `TimeInstant subClassOf hasValue only
/// (dateTime or dateTimeStamp)`, a data-range `allValuesFrom`) carries
/// `oracle: hermit` and "runs only in the CI differential (5.3)".
/// Running it under rustdl and reporting the result as a `Fail` would
/// be exactly the overstatement this harness exists to prevent: it
/// would say SULO regressed when in fact the reasoner cannot see the
/// axiom, and `load.rs` logs baseline loss for that very axiom on
/// every load.
pub const DEFERRED_TAG: &str = "oracle-hermit";

/// Why a deferred case did not run. One constant, so the text a
/// consumer reads is the same in every report format.
pub const DEFERRED_REASON: &str = "tagged `oracle-hermit`: the pinned reasoner provably cannot decide this case, so its oracle of record is the CI differential (spec 5.3), which decides it: run `sulo-testharness differential --suite <dir> --ontology <ttl> --robot <robot.jar>`, or read the report the HermiT differential CI job uploads. Not counted toward THIS run's exit code, because this run did not ask. Pass --deferred include or --deferred only to execute it under the pinned reasoner anyway.";

/// What a run does with the cases carrying [`DEFERRED_TAG`].
///
/// ONE flag with three values rather than two booleans, for the same
/// reason `--format` is one flag: `--include-deferred --only-deferred`
/// would otherwise be a request the program has to resolve by
/// silently preferring one.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum DeferredCases {
    /// Name them, count them, do not run them, and keep them out of
    /// the aggregate. The default, and what spec line 746 asks for.
    #[default]
    Skip,
    /// Run them alongside everything else, letting them set the exit
    /// code like any other case. For someone deliberately checking
    /// whether the reasoner has caught up.
    Include,
    /// Run ONLY them, under the pinned reasoner. For someone asking
    /// "what does rustdl make of the cases it is not the oracle for?".
    /// The HermiT differential does NOT use this seam: it includes
    /// every case unconditionally, because a differential that skipped
    /// the cases whose oracle of record it is would leave them checked
    /// by nothing. See `differential::run_differential`.
    Only,
}

/// A case that was selected, named, and counted, but deliberately not
/// run.
///
/// It exists as its own type rather than as a fifth `Verdict` because
/// it is not a verdict: nothing was asked of the reasoner, so there is
/// nothing to be honest or dishonest about. Keeping it out of
/// `CaseResult` also makes it structurally impossible for a deferred
/// case to reach `aggregate_cases` and set an exit code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeferredCase {
    pub id: String,
    /// The manifest this case was read from, so a reader can go and
    /// look at it.
    pub path: PathBuf,
    /// Human-readable explanation; today always [`DEFERRED_REASON`].
    pub reason: String,
}

/// What one whole-suite run was asked to do.
pub struct RunOptions<'a> {
    /// Directory to discover cases under.
    pub suite: &'a Path,
    /// Ontology used by every case that does not name its own
    /// `ontology:`. Optional, because a suite in which every case
    /// carries its own ontology needs none; a case that needs it and
    /// does not have it is a configuration error, not a silent load
    /// failure reported as a bad ontology.
    pub ontology: Option<&'a Path>,
    /// Substring matched against the manifest PATH (not the case id,
    /// which is not known until the manifest is read). Case ids in
    /// this repository are the file stems, so `--filter taxonomy`
    /// selects a group and `--filter deep-chain` selects one case.
    pub filter: Option<&'a str>,
    /// What to do with cases tagged [`DEFERRED_TAG`]. See
    /// [`DeferredCases`]; the default, `Skip`, is what spec line 746
    /// asks for.
    pub deferred: DeferredCases,
}

/// The result of one whole-suite run.
pub enum RunOutcome {
    /// Every selected case that was not deferred ran. `results` is
    /// guaranteed non-empty: `run_suite` refuses a run with nothing in
    /// it, because aggregating an empty set yields Pass and a green
    /// build that asked the reasoner nothing. That guarantee covers
    /// deferral too: a selection in which EVERY case is deferred is a
    /// configuration error, not a green run over zero cases.
    Ran {
        results: Vec<CaseResult>,
        /// Cases that were selected but deliberately not run. Carried
        /// beside the results, never inside them, so that a report can
        /// name and count every one of them while `aggregate_cases`
        /// cannot see them at all.
        deferred: Vec<DeferredCase>,
    },
    /// A configuration error: exit 2, and NOT a statement about the
    /// ontology. Carries the message to print.
    Config(String),
}

/// Aggregate a whole run's case verdicts, worst-first.
///
/// Routed through the very same `verdict::aggregate` the per-case path
/// uses, by lifting each case verdict into a `CheckOutcome`, so the
/// precedence Fail > Indeterminate > UnrefutedPass > Pass is defined
/// in exactly one place and a run can never disagree with a case about
/// what "worst" means.
///
/// `aggregate` returns Pass for an empty slice. That is safe here only
/// because `run_suite` never calls this with an empty slice: it
/// returns `Config` for a suite or filter that selected no cases, and
/// for a selection every one of whose cases was deferred. Do not
/// remove those guards on the assumption this function is defensive;
/// it is not.
///
/// Deferred cases are not `CaseResult`s at all (see [`DeferredCase`]),
/// so no deferred case can reach this function and set an exit code.
/// That is a structural property, not a filter applied here, which is
/// why there is no `if deferred` below to forget.
#[must_use]
pub fn aggregate_cases(results: &[CaseResult]) -> Verdict {
    let lifted: Vec<CheckOutcome> = results
        .iter()
        .map(|r| CheckOutcome {
            name: r.id.clone(),
            verdict: r.verdict.clone(),
            rests_on_absence: false,
        })
        .collect();
    aggregate(&lifted)
}

/// Discover, filter and load every case a run will act on.
///
/// Shared by [`run_suite`] and by `differential::run_differential`, so
/// that the four ways a run could mislead about what it checked are
/// refused identically no matter which subcommand is driving. Each
/// `Err` is a configuration error (exit 2 per spec 5.4), never a
/// verdict about the ontology: a suite that could not be read has told
/// us nothing about SULO, and reporting any of these as a failing CASE
/// would put a red mark next to SULO for a mistake in the harness's
/// own inputs.
///
/// Loading is done for EVERY selected case before the caller makes its
/// first reasoner call, so a typo in the last manifest is reported in
/// under a second rather than after the whole suite has run.
///
/// The returned vector is guaranteed non-empty.
pub fn select(
    suite: &Path,
    ontology: Option<&Path>,
    filter: Option<&str>,
) -> Result<Vec<(Case, PathBuf)>, String> {
    let discovered = discover(suite).map_err(|e| e.to_string())?;
    let discovered_count = discovered.len();

    let selected: Vec<PathBuf> = match filter {
        Some(f) => discovered
            .into_iter()
            .filter(|p| p.to_string_lossy().contains(f))
            .collect(),
        None => discovered,
    };

    // Same reasoning as `SuiteError::NoCases`, one level in: a filter
    // that matches nothing would otherwise aggregate an empty set into
    // a Pass and report a green build for a run that checked nothing.
    if selected.is_empty() {
        let filter = filter.unwrap_or_default();
        return Err(format!(
            "--filter {filter:?} matched none of the {discovered_count} case(s) under \
             {}, so this run would check nothing and still report a pass. The filter \
             is a substring match against the manifest path.",
            suite.display()
        ));
    }

    if let Some(p) = ontology
        && !p.is_file()
    {
        return Err(format!(
            "--ontology {} is not a readable file: pass the path to the ontology \
             the suite should be checked against",
            p.display()
        ));
    }

    let mut cases = Vec::with_capacity(selected.len());
    for path in &selected {
        cases.push(load_case(path).map_err(|e| e.to_string())?);
    }

    // Two cases sharing an `id` appear twice under one name in the
    // JSON `cases` array and the JUnit report, with no way for a
    // consumer to tell which verdict belongs to which manifest.
    // Discovery is by path, so nothing upstream catches it. Refused
    // rather than tolerated, for the same reason the other three
    // "this run would mislead" conditions are.
    {
        let mut seen: BTreeMap<&str, &PathBuf> = BTreeMap::new();
        for (case, path) in cases.iter().zip(&selected) {
            if let Some(first) = seen.insert(case.id.as_str(), path) {
                return Err(format!(
                    "two cases share the id {:?} ({} and {}), so a report could not say \
                     which verdict belongs to which manifest. Case ids must be unique \
                     within a suite.",
                    case.id,
                    first.display(),
                    path.display()
                ));
            }
        }
    }

    Ok(cases.into_iter().zip(selected).collect())
}

/// Discover, filter, load, and run a whole suite.
///
/// # Configuration errors abort, they are never reported as cases
///
/// A manifest that fails to load, a filter that selects nothing, an
/// `--ontology` that is not a readable file: each stops the run with
/// `Config` (exit 2). None of them is evidence about the ontology, and
/// reporting any of them as a failing CASE would put a red mark next
/// to SULO for a mistake in the harness's own inputs. Loading is done
/// for EVERY selected case before the first reasoner call, so a typo
/// in the last manifest is reported in under a second rather than
/// after the whole suite has run.
pub fn run_suite(opts: &RunOptions) -> RunOutcome {
    let selected = match select(opts.suite, opts.ontology, opts.filter) {
        Ok(s) => s,
        Err(msg) => return RunOutcome::Config(msg),
    };

    // Split off the cases whose oracle of record is not this
    // reasoner. Done AFTER loading, because the tag lives in the
    // manifest: a deferred case still has to parse, so a typo in one
    // is still exit 2 rather than a file nobody reads.
    let mut to_run: Vec<(&Case, &PathBuf)> = Vec::with_capacity(selected.len());
    let mut deferred: Vec<DeferredCase> = Vec::new();
    for (case, path) in &selected {
        let tagged = case.tags.iter().any(|t| t == DEFERRED_TAG);
        let execute = match opts.deferred {
            DeferredCases::Skip => !tagged,
            DeferredCases::Include => true,
            DeferredCases::Only => tagged,
        };
        if execute {
            to_run.push((case, path));
        } else if tagged {
            // Named and counted, never silently dropped. In `Only`
            // mode the untagged cases are NOT recorded here: the
            // operator asked for exactly the tagged ones, the same way
            // `--filter` narrows without listing what it excluded.
            deferred.push(DeferredCase {
                id: case.id.clone(),
                path: path.clone(),
                reason: DEFERRED_REASON.to_string(),
            });
        }
    }

    // The same reasoning as `SuiteError::NoCases` and the empty-filter
    // guard above, one level further in: a selection in which nothing
    // is left to run would aggregate an empty set into a Pass and
    // report a green build for a run that asked the reasoner nothing.
    // Deferral must not become a way to reach that state.
    if to_run.is_empty() {
        let err = match opts.deferred {
            DeferredCases::Only => SuiteError::NoDeferredCases {
                path: opts.suite.to_path_buf(),
                selected: selected.len(),
                tag: DEFERRED_TAG,
            },
            // `Include` runs everything, so it cannot get here with a
            // non-empty selection, and an empty selection was already
            // refused above.
            DeferredCases::Skip | DeferredCases::Include => SuiteError::AllCasesDeferred {
                path: opts.suite.to_path_buf(),
                selected: selected.len(),
                tag: DEFERRED_TAG,
            },
        };
        return RunOutcome::Config(err.to_string());
    }

    let mut results = Vec::with_capacity(to_run.len());
    for (case, path) in to_run {
        let default_ontology = match (&case.ontology, opts.ontology) {
            // The case names its own ontology, so `run_case` never
            // reads this argument. Nothing is being defaulted to the
            // empty path: the branch below is where a default is
            // actually required, and it refuses to invent one.
            (Some(_), _) => Path::new(""),
            (None, Some(p)) => p,
            (None, None) => {
                return RunOutcome::Config(format!(
                    "case {} ({}) does not set `ontology:` and --ontology was not \
                     given, so there is no ontology to check it against",
                    case.id,
                    path.display()
                ));
            }
        };
        results.push(run_case(case, default_ontology));
    }

    RunOutcome::Ran { results, deferred }
}
