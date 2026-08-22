//! Dispatching claims to the reasoner.
//!
//! Two dispatch choices are deliberate and were measured:
//!
//! * `ClassAssertion` uses `is_instance_of`, which returns the full
//!   type closure. `realize` returns only most-specific types, so it
//!   would fail every non-leaf class assertion.
//! * `ObjectPropertyAssertion` uses a `p value o` class-expression
//!   query. `inferred_object_property_values` omits reflexive
//!   self-loops, so it would fail every reflexivity check even though
//!   the entailment holds.
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

use horned_owl::model::{
    AnnotationProperty, Build, ClassExpression, Component, DataProperty, DeclareAnnotationProperty,
    DeclareDataProperty, DeclareObjectProperty, Individual, ObjectProperty,
    ObjectPropertyExpression, RcStr,
};
use horned_owl::ontology::set::SetOntology;

use crate::claim::{Claim, Literal};
use crate::verdict::{CheckOutcome, IndeterminateReason, Verdict};

const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";

/// What the case says should happen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expectation {
    Entailed,
    NotEntailed,
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

/// Does the claim hold under the reasoner? `Err` carries a message
/// for an Indeterminate verdict.
pub fn holds(onto: &SetOntology<RcStr>, claim: &Claim) -> Result<bool, String> {
    match claim {
        Claim::Subsumption { sub, sup } => {
            owl_dl_reasoner::is_subclass_of(onto, sub, sup).map_err(|e| e.to_string())
        }
        Claim::Equivalence { left, right } => {
            let a =
                owl_dl_reasoner::is_subclass_of(onto, left, right).map_err(|e| e.to_string())?;
            let b =
                owl_dl_reasoner::is_subclass_of(onto, right, left).map_err(|e| e.to_string())?;
            Ok(a && b)
        }
        Claim::Unsatisfiable { class } => owl_dl_reasoner::is_class_satisfiable(onto, class)
            .map(|sat| !sat)
            .map_err(|e| e.to_string()),
        Claim::ClassAssertion { individual, class } => {
            // Full closure, not most-specific types.
            owl_dl_reasoner::is_instance_of(onto, class, individual).map_err(|e| e.to_string())
        }
        Claim::ObjectPropertyAssertion {
            subject,
            property,
            object,
        } => {
            require_object_property(onto, property)?;
            let build: Build<RcStr> = Build::new();
            let ce = ClassExpression::ObjectHasValue {
                ope: ObjectPropertyExpression::ObjectProperty(
                    build.object_property(property.as_str()),
                ),
                i: Individual::Named(build.named_individual(object.as_str())),
            };
            let inst = owl_dl_reasoner::class_expression_instances(onto, &ce)
                .map_err(|e| e.to_string())?;
            Ok(inst.individuals().iter().any(|i| i == subject))
        }
        Claim::DataPropertyAssertion {
            subject,
            property,
            literal,
        } => {
            require_data_property(onto, property)?;
            let build: Build<RcStr> = Build::new();
            let ce = ClassExpression::DataHasValue {
                dp: build.data_property(property.as_str()),
                l: to_horned_literal(&build, literal),
            };
            let inst = owl_dl_reasoner::class_expression_instances(onto, &ce)
                .map_err(|e| e.to_string())?;
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
/// expectation it satisfies yields `UnrefutedPass`, not `Pass`.
pub fn check(onto: &SetOntology<RcStr>, claim: &Claim, expect: Expectation) -> CheckOutcome {
    let name = format!("{claim:?}");

    let verdict = match holds(onto, claim) {
        Err(msg) => Verdict::Indeterminate(IndeterminateReason::OracleError(msg)),
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
