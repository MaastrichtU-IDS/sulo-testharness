//! Dispatching claims to the reasoner.
//!
//! Dispatch choices, revised twice already and worth recording why:
//!
//! * `ClassAssertion` uses `is_instance_of`, which returns the full
//!   type closure. `realize` returns only most-specific types, so it
//!   would fail every non-leaf class assertion.
//! * `ObjectPropertyAssertion` and `DataPropertyAssertion` try the
//!   materialised `inferred_*_property_values` first. The first cut
//!   of this module went straight to a `p value o` class-expression
//!   query on every property claim, reasoning that
//!   `inferred_object_property_values` omits reflexive self-loops.
//!   That is true, but measured head to head on real SULO the
//!   class-expression path took three orders of magnitude longer
//!   (0.28s vs still running at 120s, for a claim reachable only via
//!   subproperty inference) and did not terminate in a 24-minute
//!   debug run or an 11m55s release run before being killed. The
//!   second cut narrowed the class-expression fallback to the
//!   `subject == object` self-loop case for object properties, but
//!   applied the SAME unbounded call, unguarded, to every data
//!   fallback (every language-tagged literal, every negative data
//!   claim, every datatype-IRI mismatch, every TBox-only value):
//!   `subject == object` is unsatisfiable for a data property whose
//!   object is a literal, so that guard was never coherent there and
//!   the hang was still live, just relocated.
//!
//!   The current design instead avoids the unbounded call
//!   (`class_expression_instances`, which internally runs
//!   `instances_of`'s per-individual, per-pair-deadline loop over
//!   every named individual in the ontology, the actual blow-up
//!   site) entirely. Both narrow fallbacks now go through
//!   `entailed_via_satisfiability_probe`, which reduces "is `subject`
//!   an instance of `ce`" to the standard OWL equivalence "is
//!   `{subject} ⊓ ¬ce` UNsatisfiable", answered by a single
//!   `is_class_satisfiable_with_timeout` call: a real,
//!   library-enforced, cooperatively-checked deadline, not a
//!   post-hoc measurement. This needs no worker thread (`RcStr` is
//!   `Rc<str>`, not `Send`, so a watchdog thread was never available
//!   anyway): the deadline is checked inside the tableau itself.
//!
//!   The fallbacks are narrowed to exactly the cases the materialised
//!   fast path structurally cannot express:
//!   - object properties: `subject == object` (the reflexive
//!     self-loop the materialised path never emits);
//!   - data properties: `literal.language.is_some()` (the
//!     materialised quads drop the language tag entirely), or a
//!     datatype-IRI mismatch on an otherwise-matching
//!     `(subject, property, lexical)` (the fast path compares
//!     datatype IRIs as plain strings, so `"5"^^xsd:int` vs
//!     `"5"^^xsd:integer` would wrongly miss).
//!   A plain miss outside those cases is `Ok(false)`, not a fallback
//!   call: `inferred_data_property_values` is a pure structural
//!   passthrough over asserted triples (not tableau search), so an
//!   exact non-match there is a safe negative: safe because `check`
//!   never promotes a `false` to a trustworthy `Pass`, only ever to
//!   `UnrefutedPass` for a negative expectation. Bounding every
//!   ordinary negative data claim through the reasoner instead would
//!   turn each into a multi-second `Indeterminate(Timeout)`, which
//!   recreates the "everything is Indeterminate" failure this design
//!   already rejected once. Accepted, deliberate incompleteness: a
//!   data-property value entailed only through the TBox (not
//!   asserted, not a language/datatype-representation gap) is missed
//!   by this dispatch and reported `UnrefutedPass` rather than
//!   `Pass`, same as any other negative that failed to find a proof.
//!
//! `Claim::Unsatisfiable` uses `is_class_satisfiable_with_timeout`
//! (not the unbounded `is_class_satisfiable`), mapping `Ok(None)` to
//! `Timeout`. `Claim::ClassAssertion`'s `is_instance_of` has no
//! deadline *parameter* at all; it reads
//! `RUSTDL_REALIZE_PAIR_TIMEOUT_MS` (default 750ms) and collapses a
//! deadline expiry, a max-nodes trip, and a depth bail all to a
//! trustworthy-looking `false`, discarding which one happened.
//! `ensure_realize_deadline_set` pins that environment variable to
//! this module's own `REASONER_DEADLINE` once, process-wide, so the
//! bound is the harness's choice, not the library default; a `false`
//! that took at least that long is then treated as a truncation
//! (`Timeout`), not a trustworthy negative. This is the one dispatch
//! arm that does not honour `holds_with_deadline`'s own `deadline`
//! argument (it always uses `REASONER_DEADLINE`): the environment
//! variable is process-global and set at most once, so per-call
//! values are not plumbed through it. Every other arm (the object
//! and data fallbacks, and `Unsatisfiable`) takes `deadline`
//! directly and honours it exactly.
//!
//! `ObjectPropertyValues::incomplete()` (from the object fast path)
//! and the `CeInstances::incomplete()` this module used to see (from
//! the retired unbounded fallback) are deliberately never consulted.
//! The spec measured `incomplete` firing on essentially every
//! non-EL query against real SULO, so feeding it into the verdict
//! would make `Indeterminate` the answer to nearly everything. That
//! decision stands; this note exists so dropping the flag reads as a
//! decision, not an oversight.
//!
//! Two more things this module refuses to do silently:
//!
//! * Build a literal the wrong way. horned-owl's RDF reader
//!   normalises an `xsd:string`-typed literal down to
//!   `Literal::Simple` on the way in (see its `reader.rs`), and
//!   `Simple` never compares equal to `Datatype`. `to_horned_literal`
//!   mirrors that normalisation exactly, or a claim about a plain
//!   string literal could never match anything in the ontology,
//!   regardless of whether the entailment holds. The dispatch-level
//!   regression test for this (`plain_string_literal_round_trips`)
//!   is resolved entirely by the fast path
//!   (`inferred_data_property_values` maps `Literal::Simple` to
//!   `xsd:string` on its own), so it never calls `to_horned_literal`
//!   at all: reverting the `Simple` branch back to
//!   `Datatype(xsd:string)` leaves that test green. `to_horned_literal`
//!   therefore has its own direct unit test below (`tests` module),
//!   immune to being shadowed by whichever dispatch path happens to
//!   resolve a given claim.
//! * Query a predicate that is not the right kind of property.
//!   Task 6's claim classifier has catch-all arms, so an annotation
//!   predicate (or one never declared at all) is still classified as
//!   an `ObjectPropertyAssertion` or `DataPropertyAssertion`. Handing
//!   that to the reasoner finds no instances, which would be
//!   misreported as a trustworthy Fail (positive expectation) or a
//!   silent `UnrefutedPass` (negative expectation) instead of the
//!   Indeterminate it actually is. `holds` checks the ontology's own
//!   declarations before querying.

use std::time::{Duration, Instant};

use curie::PrefixMapping;
use horned_owl::model::{
    AnnotationProperty, Build, ClassExpression, Component, DataProperty, DeclareAnnotationProperty,
    DeclareDataProperty, DeclareObjectProperty, EquivalentClasses, Individual, MutableOntology,
    ObjectProperty, ObjectPropertyExpression, RcStr,
};
use horned_owl::ontology::set::SetOntology;

use crate::claim::{Claim, Literal, parse_ce};
use crate::verdict::{CheckOutcome, IndeterminateReason, Verdict};

const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";

/// The IRI of the fresh probe class `entailed_via_satisfiability_probe`
/// defines on a per-call cloned ontology. Collision with a real
/// declared class is astronomically unlikely for a `urn:`-namespaced,
/// tool-specific IRI, and the probe ontology is discarded immediately
/// after the call, so no attempt is made to verify freshness the way
/// rustdl's own internal probes do.
const PROBE_IRI: &str = "urn:sulo-testharness:probe:q";

/// Bound on the reasoner's per-query work: the harness's own choice,
/// used everywhere a deadline can be passed explicitly, and pinned
/// into `RUSTDL_REALIZE_PAIR_TIMEOUT_MS` for the one dispatch arm
/// (`ClassAssertion`) that cannot take a deadline parameter. See the
/// module doc for why 15s and for which arms this actually bounds.
const REASONER_DEADLINE: Duration = Duration::from_secs(15);

/// What the case says should happen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expectation {
    Entailed,
    NotEntailed,
}

/// Why `holds` could not produce a boolean answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OracleFailure {
    /// The reasoner's time budget was exceeded. Never treated as
    /// Fail or Pass; `check` maps this to
    /// `IndeterminateReason::Timeout`.
    Timeout,
    /// Any other reasoner or declaration error, carrying a message.
    Error(String),
}

impl std::fmt::Display for OracleFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OracleFailure::Timeout => write!(f, "reasoner exceeded its time budget"),
            OracleFailure::Error(msg) => write!(f, "{msg}"),
        }
    }
}

/// What a predicate IRI is actually declared as, if anything.
enum PropertyKind {
    Object,
    Data,
    Annotation,
    Undeclared,
}

/// Look up how `iri` is declared in `onto`. Unrelated declarations
/// (classes, individuals, other properties) are ignored; only the
/// three property-declaration kinds matter for dispatch.
fn declared_property_kind(onto: &SetOntology<RcStr>, iri: &str) -> PropertyKind {
    for ac in onto.iter() {
        match &ac.component {
            Component::DeclareObjectProperty(DeclareObjectProperty(ObjectProperty(op)))
                if op.as_ref() == iri =>
            {
                return PropertyKind::Object;
            }
            Component::DeclareDataProperty(DeclareDataProperty(DataProperty(dp)))
                if dp.as_ref() == iri =>
            {
                return PropertyKind::Data;
            }
            Component::DeclareAnnotationProperty(DeclareAnnotationProperty(
                AnnotationProperty(ap),
            )) if ap.as_ref() == iri => {
                return PropertyKind::Annotation;
            }
            _ => {}
        }
    }
    PropertyKind::Undeclared
}

/// Fail loudly, naming the predicate, unless it is declared as an
/// object property.
fn require_object_property(onto: &SetOntology<RcStr>, iri: &str) -> Result<(), String> {
    match declared_property_kind(onto, iri) {
        PropertyKind::Object => Ok(()),
        PropertyKind::Data => Err(format!(
            "{iri} is declared as a data property, not an object property"
        )),
        PropertyKind::Annotation => Err(format!(
            "{iri} is declared as an annotation property, not an object property"
        )),
        PropertyKind::Undeclared => Err(format!(
            "{iri} is not declared as a property in the ontology"
        )),
    }
}

/// Fail loudly, naming the predicate, unless it is declared as a data
/// property.
fn require_data_property(onto: &SetOntology<RcStr>, iri: &str) -> Result<(), String> {
    match declared_property_kind(onto, iri) {
        PropertyKind::Data => Ok(()),
        PropertyKind::Object => Err(format!(
            "{iri} is declared as an object property, not a data property"
        )),
        PropertyKind::Annotation => Err(format!(
            "{iri} is declared as an annotation property, not a data property"
        )),
        PropertyKind::Undeclared => Err(format!(
            "{iri} is not declared as a property in the ontology"
        )),
    }
}

/// Set once, process-wide: see the module doc for why `ClassAssertion`
/// cannot take a per-call deadline the way the other arms do.
static SET_REALIZE_DEADLINE: std::sync::Once = std::sync::Once::new();

fn ensure_realize_deadline_set() {
    SET_REALIZE_DEADLINE.call_once(|| {
        // SAFETY: `call_once` guarantees this runs exactly once, and
        // no other code in this process reads or writes
        // `RUSTDL_REALIZE_PAIR_TIMEOUT_MS`, so there is no concurrent
        // access to race against.
        unsafe {
            std::env::set_var(
                "RUSTDL_REALIZE_PAIR_TIMEOUT_MS",
                REASONER_DEADLINE.as_millis().to_string(),
            );
        }
    });
}

/// The one bounded-probe entry point in this module: define a fresh
/// class `Q ≡ ce` on a cloned ontology and ask only its
/// satisfiability, via `is_class_satisfiable_with_timeout`
/// (`Ok(None)` on a genuine, cooperatively-checked deadline expiry).
/// Every class-expression question this module answers, the two
/// narrow dispatch fallbacks below and the three Manchester checks
/// (`check_subsumption_expr`, `check_instance_expr`,
/// `check_satisfiable_expr`), is reduced to exactly this call, so
/// this is the single place that ever touches
/// `is_class_satisfiable_with_timeout` and the only place a change to
/// the probing strategy needs to happen. None of this module calls
/// the unbounded `class_expression_entailed_subclass`,
/// `class_expression_instances`, or `class_expression_satisfiable`
/// (which itself has no deadline parameter at all): those internally
/// loop or search without a cooperative deadline check; see the
/// module doc for the 24-minute hang that motivated retiring them.
fn probe_satisfiable(
    onto: &SetOntology<RcStr>,
    ce: ClassExpression<RcStr>,
    deadline: Duration,
) -> Result<bool, OracleFailure> {
    let build: Build<RcStr> = Build::new();
    let mut probed = onto.clone();
    probed.insert(EquivalentClasses(vec![
        ClassExpression::Class(build.class(PROBE_IRI)),
        ce,
    ]));

    match owl_dl_reasoner::is_class_satisfiable_with_timeout(&probed, PROBE_IRI, deadline) {
        Ok(Some(sat)) => Ok(sat),
        Ok(None) => Err(OracleFailure::Timeout),
        Err(e) => Err(OracleFailure::Error(e.to_string())),
    }
}

/// Bounded check for "is `subject` an instance of `ce`?". Standard OWL
/// reduction: `subject` is an instance of `ce` in every model iff
/// `{subject} ⊓ ¬ce` is UNsatisfiable, so this builds that intersection
/// and hands it to `probe_satisfiable`. This is what replaces the
/// unbounded `class_expression_instances` (whose internal
/// `instances_of` loops over every named individual in the ontology;
/// see the module doc) for the two narrow dispatch fallbacks that
/// still need a real reasoner call, and is reused as-is by
/// `check_instance_expr` below: membership of a named individual in a
/// Manchester expression is the exact same shape.
fn entailed_via_satisfiability_probe(
    onto: &SetOntology<RcStr>,
    subject: &str,
    ce: ClassExpression<RcStr>,
    deadline: Duration,
) -> Result<bool, OracleFailure> {
    let build: Build<RcStr> = Build::new();
    let subject_nominal =
        ClassExpression::ObjectOneOf(vec![Individual::Named(build.named_individual(subject))]);
    let probe_definition = ClassExpression::ObjectIntersectionOf(vec![
        subject_nominal,
        ClassExpression::ObjectComplementOf(Box::new(ce)),
    ]);

    probe_satisfiable(onto, probe_definition, deadline).map(|sat| !sat)
}

/// Does the claim hold under the reasoner, using this module's
/// default `REASONER_DEADLINE`?
pub fn holds(onto: &SetOntology<RcStr>, claim: &Claim) -> Result<bool, OracleFailure> {
    holds_with_deadline(onto, claim, REASONER_DEADLINE)
}

/// As `holds`, but with an explicit `deadline` in place of the
/// module's default. A seam for testing the Timeout path
/// deterministically (a zero deadline forces it on any arm that
/// genuinely needs tableau work) without relying on a real reasoner
/// call being slow. `ClassAssertion` is the one exception: see the
/// module doc.
pub fn holds_with_deadline(
    onto: &SetOntology<RcStr>,
    claim: &Claim,
    deadline: Duration,
) -> Result<bool, OracleFailure> {
    match claim {
        Claim::Subsumption { sub, sup } => owl_dl_reasoner::is_subclass_of(onto, sub, sup)
            .map_err(|e| OracleFailure::Error(e.to_string())),
        Claim::Equivalence { left, right } => {
            let a = owl_dl_reasoner::is_subclass_of(onto, left, right)
                .map_err(|e| OracleFailure::Error(e.to_string()))?;
            let b = owl_dl_reasoner::is_subclass_of(onto, right, left)
                .map_err(|e| OracleFailure::Error(e.to_string()))?;
            Ok(a && b)
        }
        Claim::Unsatisfiable { class } => {
            match owl_dl_reasoner::is_class_satisfiable_with_timeout(onto, class, deadline) {
                Ok(Some(sat)) => Ok(!sat),
                Ok(None) => Err(OracleFailure::Timeout),
                Err(e) => Err(OracleFailure::Error(e.to_string())),
            }
        }
        Claim::ClassAssertion { individual, class } => {
            // Does not honour `deadline`: see the module doc.
            ensure_realize_deadline_set();
            let start = Instant::now();
            let result = owl_dl_reasoner::is_instance_of(onto, class, individual)
                .map_err(|e| OracleFailure::Error(e.to_string()))?;
            if !result && start.elapsed() >= REASONER_DEADLINE {
                // is_instance_of's public API collapses a deadline
                // expiry, a max-nodes trip, and a depth bail all to
                // `false`, discarding which one happened. A `false`
                // that took at least our own deadline is treated as
                // that truncation, not a trustworthy negative.
                return Err(OracleFailure::Timeout);
            }
            Ok(result)
        }
        Claim::ObjectPropertyAssertion {
            subject,
            property,
            object,
        } => {
            require_object_property(onto, property).map_err(OracleFailure::Error)?;

            // Fast path: materialised entailments, deadline-bounded.
            let start = Instant::now();
            let values = owl_dl_reasoner::inferred_object_property_values(onto, Some(deadline))
                .map_err(|e| OracleFailure::Error(e.to_string()))?;
            if values
                .triples()
                .iter()
                .any(|(s, p, o)| s == subject && p == property && o == object)
            {
                return Ok(true);
            }
            if start.elapsed() >= deadline {
                return Err(OracleFailure::Timeout);
            }

            // Narrow fallback: the reflexive self-loop gap that the
            // materialised path does not emit. Restricting this to
            // subject == object is what keeps it bounded to exactly
            // the case that needs it; see the module doc.
            if subject == object {
                let build: Build<RcStr> = Build::new();
                let ce = ClassExpression::ObjectHasValue {
                    ope: ObjectPropertyExpression::ObjectProperty(
                        build.object_property(property.as_str()),
                    ),
                    i: Individual::Named(build.named_individual(object.as_str())),
                };
                return entailed_via_satisfiability_probe(onto, subject, ce, deadline);
            }

            // Otherwise: not found in the materialised closure and
            // not a self-loop. Safe to treat as not entailed; see the
            // module doc for why this is not a fallback trigger.
            Ok(false)
        }
        Claim::DataPropertyAssertion {
            subject,
            property,
            literal,
        } => {
            require_data_property(onto, property).map_err(OracleFailure::Error)?;

            // Fast path: materialised entailments. No deadline
            // parameter exists here because this call is a pure
            // structural passthrough over asserted data-property
            // triples, not a tableau search, so it has no comparable
            // blow-up risk.
            let values = owl_dl_reasoner::inferred_data_property_values(onto)
                .map_err(|e| OracleFailure::Error(e.to_string()))?;
            let lexical = &literal.lexical;
            let datatype = literal.datatype.as_str();
            let language_tagged = literal.language.is_some();

            if !language_tagged
                && values.quads().iter().any(|(s, p, lex, dt)| {
                    s == subject && p == property && lex == lexical && dt == datatype
                })
            {
                return Ok(true);
            }

            // Narrow fallback trigger: only the two cases the fast
            // path structurally cannot express (see the module doc).
            // A datatype-IRI mismatch is detected as any remaining
            // (subject, property, lexical) match once the exact
            // match above has already missed.
            let datatype_mismatch_on_match = !language_tagged
                && values
                    .quads()
                    .iter()
                    .any(|(s, p, lex, _dt)| s == subject && p == property && lex == lexical);

            if !language_tagged && !datatype_mismatch_on_match {
                return Ok(false);
            }

            let build: Build<RcStr> = Build::new();
            let ce = ClassExpression::DataHasValue {
                dp: build.data_property(property.as_str()),
                l: to_horned_literal(&build, literal),
            };
            entailed_via_satisfiability_probe(onto, subject, ce, deadline)
        }
    }
}

/// Mirror horned-owl's RDF reader normalisation exactly: a language
/// tag wins, then a literal typed `xsd:string` becomes `Simple`
/// (never `Datatype`), and only then does a real datatype IRI apply.
/// `Simple` and `Datatype` are distinct enum variants with no
/// normalisation between them at comparison time, so getting this
/// wrong means a claim about a plain string literal can never match
/// what is actually in the ontology. See the `tests` module below for
/// the direct unit test: the dispatch-level regression test for this
/// no longer exercises this function at all (it is resolved by the
/// data fast path instead), so this is the only test that would catch
/// a regression here.
fn to_horned_literal(build: &Build<RcStr>, lit: &Literal) -> horned_owl::model::Literal<RcStr> {
    if let Some(lang) = &lit.language {
        horned_owl::model::Literal::Language {
            literal: lit.lexical.clone(),
            lang: lang.clone(),
        }
    } else if lit.datatype == XSD_STRING {
        horned_owl::model::Literal::Simple {
            literal: lit.lexical.clone(),
        }
    } else {
        horned_owl::model::Literal::Datatype {
            literal: lit.lexical.clone(),
            datatype_iri: build.iri(lit.datatype.as_str()),
        }
    }
}

/// Turn a raw bool-plus-expectation into a verdict. The single source
/// of truth for the whole harness's central asymmetry, shared by
/// `check` and by all three Manchester class-expression checks below.
///
/// The asymmetry is the whole point. A reasoner that says "entailed"
/// (or "held") is trustworthy because it is sound. A reasoner that
/// says "not entailed" has only failed to find a proof, so a negative
/// expectation it satisfies yields `UnrefutedPass`, not `Pass`.
/// Timeouts never reach this function: they are handled by the caller
/// and mapped to `Indeterminate` before `held` is known.
fn verdict_for(held: bool, expect: Expectation, what: &str) -> Verdict {
    match (held, expect) {
        (true, Expectation::Entailed) => Verdict::Pass,
        (true, Expectation::NotEntailed) => {
            Verdict::Fail(format!("expected NOT to hold, but it does: {what}"))
        }
        (false, Expectation::Entailed) => Verdict::Fail(format!(
            "expected to hold, but no proof was found: {what}. \
             Incompleteness is a possible cause; the CI differential settles it."
        )),
        (false, Expectation::NotEntailed) => Verdict::UnrefutedPass,
    }
}

/// Run one claim against its expectation and produce a verdict. See
/// `verdict_for` for the asymmetry this delegates to.
pub fn check(onto: &SetOntology<RcStr>, claim: &Claim, expect: Expectation) -> CheckOutcome {
    let name = format!("{claim:?}");

    let verdict = match holds(onto, claim) {
        Err(OracleFailure::Timeout) => Verdict::Indeterminate(IndeterminateReason::Timeout),
        Err(OracleFailure::Error(msg)) => {
            Verdict::Indeterminate(IndeterminateReason::OracleError(msg))
        }
        Ok(held) => verdict_for(held, expect, &name),
    };

    CheckOutcome { name, verdict }
}

/// Is `sub_expr` subsumed by `sup_expr`? Reduced to the standard OWL
/// equivalence: `sub_expr ⊑ sup_expr` in every model iff
/// `sub_expr ⊓ ¬sup_expr` is UNsatisfiable. Never calls the unbounded
/// `class_expression_entailed_subclass`; see `probe_satisfiable`.
pub fn check_subsumption_expr(
    onto: &SetOntology<RcStr>,
    sub_expr: &str,
    sup_expr: &str,
    expect: Expectation,
    pm: &PrefixMapping,
) -> CheckOutcome {
    let what = format!("{sub_expr} subClassOf {sup_expr}");
    let (sub, sup) = match (parse_ce(sub_expr, pm), parse_ce(sup_expr, pm)) {
        (Ok(a), Ok(b)) => (a, b),
        (Err(e), _) | (_, Err(e)) => {
            return CheckOutcome {
                name: what,
                verdict: Verdict::Indeterminate(IndeterminateReason::OracleError(e.to_string())),
            };
        }
    };

    let intersection = ClassExpression::ObjectIntersectionOf(vec![
        sub,
        ClassExpression::ObjectComplementOf(Box::new(sup)),
    ]);

    let verdict = match probe_satisfiable(onto, intersection, REASONER_DEADLINE) {
        Ok(sat) => verdict_for(!sat, expect, &what),
        Err(OracleFailure::Timeout) => Verdict::Indeterminate(IndeterminateReason::Timeout),
        Err(OracleFailure::Error(msg)) => {
            Verdict::Indeterminate(IndeterminateReason::OracleError(msg))
        }
    };

    CheckOutcome {
        name: what,
        verdict,
    }
}

/// Is `individual` provably in `expr`? Exactly the shape Task 7 uses
/// for its object-property fallback (`entailed_via_satisfiability_probe`),
/// reused directly rather than duplicated.
pub fn check_instance_expr(
    onto: &SetOntology<RcStr>,
    individual: &str,
    expr: &str,
    expect: Expectation,
    pm: &PrefixMapping,
) -> CheckOutcome {
    let what = format!("{individual} instanceOf {expr}");
    let ce = match parse_ce(expr, pm) {
        Ok(c) => c,
        Err(e) => {
            return CheckOutcome {
                name: what,
                verdict: Verdict::Indeterminate(IndeterminateReason::OracleError(e.to_string())),
            };
        }
    };

    let verdict = match entailed_via_satisfiability_probe(onto, individual, ce, REASONER_DEADLINE) {
        Ok(held) => verdict_for(held, expect, &what),
        Err(OracleFailure::Timeout) => Verdict::Indeterminate(IndeterminateReason::Timeout),
        Err(OracleFailure::Error(msg)) => {
            Verdict::Indeterminate(IndeterminateReason::OracleError(msg))
        }
    };

    CheckOutcome {
        name: what,
        verdict,
    }
}

/// Does `expr` have a model? Guards a pattern going unsatisfiable.
/// `expect` follows the same `verdict_for` asymmetry as the other two
/// checks: `Entailed` means "expect satisfiable" (the common case,
/// e.g. guarding a competency-question pattern), `NotEntailed` means
/// "expect unsatisfiable". A direct, always-expect-satisfiable claim
/// about a *named* class already has a dedicated path
/// (`Claim::Unsatisfiable`, via `holds`); this is for a raw Manchester
/// expression with no class declaration behind it.
pub fn check_satisfiable_expr(
    onto: &SetOntology<RcStr>,
    expr: &str,
    expect: Expectation,
    pm: &PrefixMapping,
) -> CheckOutcome {
    let what = format!("satisfiable: {expr}");
    let ce = match parse_ce(expr, pm) {
        Ok(c) => c,
        Err(e) => {
            return CheckOutcome {
                name: what,
                verdict: Verdict::Indeterminate(IndeterminateReason::OracleError(e.to_string())),
            };
        }
    };

    let verdict = match probe_satisfiable(onto, ce, REASONER_DEADLINE) {
        Ok(sat) => verdict_for(sat, expect, &what),
        Err(OracleFailure::Timeout) => Verdict::Indeterminate(IndeterminateReason::Timeout),
        Err(OracleFailure::Error(msg)) => {
            Verdict::Indeterminate(IndeterminateReason::OracleError(msg))
        }
    };

    CheckOutcome {
        name: what,
        verdict,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Direct unit test of `to_horned_literal`'s three shapes. No
    /// dispatch path can shadow this: `plain_string_literal_round_trips`
    /// in `tests/oracle.rs` is resolved entirely by the data fast path
    /// and never calls this function at all (see its doc comment).
    #[test]
    fn to_horned_literal_mirrors_the_reader_normalisation() {
        let build: Build<RcStr> = Build::new();

        let language_tagged = Literal {
            lexical: "bonjour".into(),
            datatype: "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString".into(),
            language: Some("fr".into()),
        };
        assert_eq!(
            to_horned_literal(&build, &language_tagged),
            horned_owl::model::Literal::Language {
                literal: "bonjour".into(),
                lang: "fr".into(),
            },
            "a language tag must win over datatype, producing Literal::Language"
        );

        let plain_string = Literal {
            lexical: "hello".into(),
            datatype: XSD_STRING.into(),
            language: None,
        };
        assert_eq!(
            to_horned_literal(&build, &plain_string),
            horned_owl::model::Literal::Simple {
                literal: "hello".into(),
            },
            "an untagged xsd:string literal must become Literal::Simple, not Literal::Datatype"
        );

        let typed = Literal {
            lexical: "3.5".into(),
            datatype: "http://www.w3.org/2001/XMLSchema#double".into(),
            language: None,
        };
        assert_eq!(
            to_horned_literal(&build, &typed),
            horned_owl::model::Literal::Datatype {
                literal: "3.5".into(),
                datatype_iri: build.iri("http://www.w3.org/2001/XMLSchema#double"),
            },
            "a real, non-string datatype must become Literal::Datatype"
        );
    }
}
