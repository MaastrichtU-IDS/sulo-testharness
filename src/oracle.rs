//! Dispatching claims to the reasoner.
//!
//! Dispatch choices, revised once already and worth recording why:
//!
//! * `ClassAssertion` uses `is_instance_of`, which returns the full
//!   type closure. `realize` returns only most-specific types, so it
//!   would fail every non-leaf class assertion.
//! * `ObjectPropertyAssertion` and `DataPropertyAssertion` try the
//!   materialised `inferred_*_property_values` first, and only fall
//!   back to a `p value o` class-expression query for the narrow
//!   reflexive self-loop case the materialised path cannot see. The
//!   first cut of this module went straight to the class-expression
//!   query on every property claim, reasoning that
//!   `inferred_object_property_values` omits reflexive self-loops.
//!   That is true, but measured head to head on real SULO the
//!   class-expression path took three orders of magnitude longer
//!   (0.28s vs still running at 120s, for a claim reachable only via
//!   subproperty inference) and did not terminate in a 24-minute
//!   debug run or an 11m55s release run before being killed. Trading
//!   a narrow, known gap (self-loops) for an effectively unbounded
//!   one was the wrong trade. The self-loop gap is exactly the case
//!   `subject == object`, which is cheap to detect, so a fast
//!   materialised path plus a narrow fallback restricted to that one
//!   case gets both properties: the fast path is sound (materialised
//!   entailments), and "not found by either" is safe to treat as not
//!   entailed because a negative expectation satisfied that way only
//!   ever yields `UnrefutedPass`, never a trustworthy `Pass`.
//!
//! Two more things this module refuses to do silently:
//!
//! * Build a literal the wrong way. horned-owl's RDF reader
//!   normalises an `xsd:string`-typed literal down to
//!   `Literal::Simple` on the way in (see its `reader.rs`), and
//!   `Simple` never compares equal to `Datatype`. `to_horned_literal`
//!   mirrors that normalisation exactly, or a claim about a plain
//!   string literal could never match anything in the ontology,
//!   regardless of whether the entailment holds.
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

use horned_owl::model::{
    AnnotationProperty, Build, ClassExpression, Component, DataProperty, DeclareAnnotationProperty,
    DeclareDataProperty, DeclareObjectProperty, Individual, ObjectProperty,
    ObjectPropertyExpression, RcStr,
};
use horned_owl::ontology::set::SetOntology;

use crate::claim::{Claim, Literal};
use crate::verdict::{CheckOutcome, IndeterminateReason, Verdict};

const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";

/// Bound on the reasoner's per-query work. Passed as
/// `inferred_object_property_values`'s own `pair_deadline` (a real,
/// library-enforced deadline on each candidate-pair extension probe),
/// and used as a post-hoc wall-clock guard around the narrow
/// class-expression fallback, which the pinned rustdl v0.4.22 exposes
/// no deadline parameter for at all. That fallback's guard cannot
/// preempt a call already in progress: `RcStr` is `Rc<str>`, which is
/// not `Send`, so the call cannot be moved to a watchdog thread. It
/// only labels a slow-but-completed answer as `Timeout` rather than
/// silently trusting it. This is safe in practice because the
/// fallback is now reached only for the reflexive `subject == object`
/// case, which was measured fast (0.28s-class) on real SULO; the
/// unbounded case that used to hang is resolved by never reaching
/// this call for it at all.
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

/// Does the claim hold under the reasoner? `Err` carries either a
/// genuine timeout or a message for an Indeterminate verdict.
pub fn holds(onto: &SetOntology<RcStr>, claim: &Claim) -> Result<bool, OracleFailure> {
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
        Claim::Unsatisfiable { class } => owl_dl_reasoner::is_class_satisfiable(onto, class)
            .map(|sat| !sat)
            .map_err(|e| OracleFailure::Error(e.to_string())),
        Claim::ClassAssertion { individual, class } => {
            // Full closure, not most-specific types.
            owl_dl_reasoner::is_instance_of(onto, class, individual)
                .map_err(|e| OracleFailure::Error(e.to_string()))
        }
        Claim::ObjectPropertyAssertion {
            subject,
            property,
            object,
        } => {
            require_object_property(onto, property).map_err(OracleFailure::Error)?;

            // Fast path: materialised entailments, deadline-bounded.
            let start = Instant::now();
            let values =
                owl_dl_reasoner::inferred_object_property_values(onto, Some(REASONER_DEADLINE))
                    .map_err(|e| OracleFailure::Error(e.to_string()))?;
            if values
                .triples()
                .iter()
                .any(|(s, p, o)| s == subject && p == property && o == object)
            {
                return Ok(true);
            }
            if start.elapsed() >= REASONER_DEADLINE {
                return Err(OracleFailure::Timeout);
            }

            // Narrow fallback: the reflexive self-loop gap that the
            // materialised path does not emit. Restricting this to
            // subject == object is what keeps it fast; see the
            // module doc for the case that made the general query
            // unbounded.
            if subject == object {
                let fallback_start = Instant::now();
                let build: Build<RcStr> = Build::new();
                let ce = ClassExpression::ObjectHasValue {
                    ope: ObjectPropertyExpression::ObjectProperty(
                        build.object_property(property.as_str()),
                    ),
                    i: Individual::Named(build.named_individual(object.as_str())),
                };
                let inst = owl_dl_reasoner::class_expression_instances(onto, &ce)
                    .map_err(|e| OracleFailure::Error(e.to_string()))?;
                if fallback_start.elapsed() >= REASONER_DEADLINE {
                    return Err(OracleFailure::Timeout);
                }
                return Ok(inst.individuals().iter().any(|i| i == subject));
            }

            // Otherwise: not found in the materialised closure and
            // not a self-loop. Incompleteness beyond the self-loop
            // case degrades gracefully here, not silently: a
            // negative expectation satisfied by this `false` yields
            // `UnrefutedPass`, never `Pass`.
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
            let datatype = if literal.language.is_some() {
                // The materialised quads drop the language tag
                // entirely (see inferred_data_property_values's own
                // doc), so a language-tagged claim can never be
                // confirmed by the fast path; fall through to the
                // class-expression query below, which builds the
                // literal correctly via to_horned_literal.
                None
            } else if literal.datatype == XSD_STRING {
                Some(XSD_STRING)
            } else {
                Some(literal.datatype.as_str())
            };
            if let Some(datatype) = datatype {
                if values.quads().iter().any(|(s, p, lex, dt)| {
                    s == subject && p == property && lex == lexical && dt == datatype
                }) {
                    return Ok(true);
                }
            }

            // Fallback: the class-expression query, for
            // language-tagged literals and anything the structural
            // passthrough could not represent. No deadline parameter
            // exists for this call either; guarded post-hoc for the
            // same reason as the object-property fallback above.
            let fallback_start = Instant::now();
            let build: Build<RcStr> = Build::new();
            let ce = ClassExpression::DataHasValue {
                dp: build.data_property(property.as_str()),
                l: to_horned_literal(&build, literal),
            };
            let inst = owl_dl_reasoner::class_expression_instances(onto, &ce)
                .map_err(|e| OracleFailure::Error(e.to_string()))?;
            if fallback_start.elapsed() >= REASONER_DEADLINE {
                return Err(OracleFailure::Timeout);
            }
            Ok(inst.individuals().iter().any(|i| i == subject))
        }
    }
}

/// Mirror horned-owl's RDF reader normalisation exactly: a language
/// tag wins, then a literal typed `xsd:string` becomes `Simple`
/// (never `Datatype`), and only then does a real datatype IRI apply.
/// `Simple` and `Datatype` are distinct enum variants with no
/// normalisation between them at comparison time, so getting this
/// wrong means a claim about a plain string literal can never match
/// what is actually in the ontology.
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

/// Run one claim against its expectation and produce a verdict.
///
/// The asymmetry is the whole point. A reasoner that says "entailed"
/// is trustworthy because it is sound. A reasoner that says "not
/// entailed" has only failed to find a proof, so a negative
/// expectation it satisfies yields `UnrefutedPass`, not `Pass`. A
/// timeout is neither: it is never promoted to Fail or Pass, only to
/// Indeterminate.
pub fn check(onto: &SetOntology<RcStr>, claim: &Claim, expect: Expectation) -> CheckOutcome {
    let name = format!("{claim:?}");

    let verdict = match holds(onto, claim) {
        Err(OracleFailure::Timeout) => Verdict::Indeterminate(IndeterminateReason::Timeout),
        Err(OracleFailure::Error(msg)) => {
            Verdict::Indeterminate(IndeterminateReason::OracleError(msg))
        }
        Ok(true) => match expect {
            Expectation::Entailed => Verdict::Pass,
            Expectation::NotEntailed => Verdict::Fail(format!(
                "expected NOT entailed, but it is entailed: {claim:?}"
            )),
        },
        Ok(false) => match expect {
            Expectation::Entailed => Verdict::Fail(format!(
                "expected entailed, but no proof was found: {claim:?}. \
                 Incompleteness is a possible cause; the CI differential settles it."
            )),
            Expectation::NotEntailed => Verdict::UnrefutedPass,
        },
    };

    CheckOutcome { name, verdict }
}
