//! Verdicts and their precedence.
//!
//! Four outcomes, not two. `UnrefutedPass` exists because a
//! sound-but-incomplete reasoner reporting "not entailed" for a
//! negative test has not proved the non-entailment, only failed to
//! refute it. Reporting that as an ordinary Pass would overstate what
//! the harness knows.

/// Why a check could not be decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndeterminateReason {
    /// The reasoner exceeded the case's time budget.
    Timeout,
    /// An axiom was lost on the way in, so a "not entailed" answer is
    /// not meaningful. Carries a human-readable description.
    AxiomLoss(String),
    /// The reasoner returned an error for this query.
    OracleError(String),
}

/// The outcome of a single check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Trustworthy pass, guaranteed by the reasoner's soundness.
    Pass,
    /// A negative expectation the reasoner failed to refute. Not a
    /// proof of non-entailment. Does not fail the build.
    UnrefutedPass,
    /// Undecided. Never silently promoted or demoted.
    Indeterminate(IndeterminateReason),
    /// Trustworthy failure, carrying an explanation.
    Fail(String),
}

impl Verdict {
    /// Higher rank wins when aggregating.
    fn rank(&self) -> u8 {
        match self {
            Verdict::Pass => 0,
            Verdict::UnrefutedPass => 1,
            Verdict::Indeterminate(_) => 2,
            Verdict::Fail(_) => 3,
        }
    }
}

/// One named check and how it came out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckOutcome {
    pub name: String,
    pub verdict: Verdict,
    /// True when this outcome's meaning depends on something being
    /// ABSENT from what the reasoner or the materialised store could
    /// produce, rather than on something positively found.
    ///
    /// `suite::downgrade_for_loss` recognises the entailment-path
    /// absence shapes structurally, from the verdict text
    /// (`oracle::NO_PROOF_MARKER`) or the gate's own check name. The
    /// competency-question path has neither: a `cq` verdict is built
    /// by `rows::compare` over a materialised store, so its messages
    /// carry no marker and its name is not a `GATE_*` constant. This
    /// flag is how `cq` (or any future check kind with the same
    /// problem) declares the dependency explicitly instead of hiding
    /// it behind a string match on the check name, which would drift
    /// the moment the name format changed. See `cq::check_cq` for
    /// which competency-question outcomes set it and why.
    pub rests_on_absence: bool,
}

/// Combine outcomes worst-first. An empty set passes.
#[must_use]
pub fn aggregate(outcomes: &[CheckOutcome]) -> Verdict {
    outcomes
        .iter()
        .map(|o| o.verdict.clone())
        .max_by_key(Verdict::rank)
        .unwrap_or(Verdict::Pass)
}

/// Map a verdict to its process exit code. Codes 2, 4, and 5 are
/// raised by the caller, not derived from a verdict.
#[must_use]
pub fn exit_code(v: &Verdict) -> i32 {
    match v {
        Verdict::Pass | Verdict::UnrefutedPass => 0,
        Verdict::Fail(_) => 1,
        Verdict::Indeterminate(_) => 3,
    }
}

/// The exit code for a whole run, honouring `--allow-indeterminate`.
///
/// Spec 5.4 says exit `3` on "any Indeterminate, unless
/// `--allow-indeterminate`". The flag exists for the consumer who hits
/// a genuine reasoner timeout and has, with the composite action
/// failing the step on 3, no other supported way forward.
///
/// It can never suppress a `Fail`, and that is structural rather than
/// a rule to remember: the match below lowers the code only for the
/// `Indeterminate` arm, and `aggregate` returns `Fail` for any set
/// containing one (Fail outranks Indeterminate). So a run holding both
/// a Fail and an Indeterminate aggregates to `Fail` and exits 1 with
/// the flag set, exactly as without it. Written as a match on the
/// verdict rather than as `if code == 3 { 0 }` for that reason: the
/// numeric form would silently start suppressing any future verdict
/// that also mapped to 3.
///
/// Indeterminates stay fully visible in every report format either
/// way. This function decides an exit code; it hides nothing.
#[must_use]
pub fn run_exit_code(v: &Verdict, allow_indeterminate: bool) -> i32 {
    match v {
        Verdict::Indeterminate(_) if allow_indeterminate => 0,
        other => exit_code(other),
    }
}
