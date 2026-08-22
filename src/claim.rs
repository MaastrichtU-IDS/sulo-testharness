//! Turning an author's Turtle fragment into questions a reasoner can
//! answer.
//!
//! The fragment is real Turtle, parsed with the suite prefix map
//! prepended, then each triple is classified by shape. A triple
//! matching no shape is an error: silently skipping it would report a
//! green for a check that never ran.

use curie::PrefixMapping;
use horned_owl::model::{Build, ClassExpression, RcStr};
use oxrdf::{NamedOrBlankNode, Term, Triple};
use oxrdfio::{RdfFormat, RdfParser};

use crate::prefixes::PrefixError;

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_SUBCLASSOF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const OWL_EQUIVALENTCLASS: &str = "http://www.w3.org/2002/07/owl#equivalentClass";
const OWL_NOTHING: &str = "http://www.w3.org/2002/07/owl#Nothing";

/// An RDF literal, compared by term rather than by string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Literal {
    pub lexical: String,
    pub datatype: String,
    pub language: Option<String>,
}

/// A single checkable assertion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Claim {
    Subsumption {
        sub: String,
        sup: String,
    },
    Equivalence {
        left: String,
        right: String,
    },
    Unsatisfiable {
        class: String,
    },
    ClassAssertion {
        individual: String,
        class: String,
    },
    ObjectPropertyAssertion {
        subject: String,
        property: String,
        object: String,
    },
    DataPropertyAssertion {
        subject: String,
        property: String,
        literal: Literal,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum ClaimError {
    #[error("fragment is not valid Turtle: {0}")]
    Syntax(String),
    #[error("blank nodes cannot be used in a claim ({0}); use a skolemised IRI")]
    BlankNode(String),
    #[error("prefix problem: {0}")]
    Prefix(#[from] PrefixError),
    #[error("a literal object is only meaningful for a data property, got predicate {0}")]
    LiteralWithNonDataPredicate(String),
    #[error("invalid Manchester class expression '{expr}': {message}")]
    ManchesterSyntax { expr: String, message: String },
}

/// Parse a fragment into claims. `pm` supplies the `@prefix` header.
pub fn parse_fragment(fragment: &str, pm: &PrefixMapping) -> Result<Vec<Claim>, ClaimError> {
    let mut doc = String::new();
    for (prefix, iri) in pm.mappings() {
        doc.push_str(&format!("@prefix {prefix}: <{iri}> .\n"));
    }
    doc.push_str(fragment);

    let parser = RdfParser::from_format(RdfFormat::Turtle);
    let mut claims = Vec::new();

    for quad in parser.for_reader(doc.as_bytes()) {
        let quad = quad.map_err(|e| ClaimError::Syntax(e.to_string()))?;
        let triple: Triple = quad.into();
        claims.push(classify(&triple)?);
    }

    Ok(claims)
}

/// Parse a Manchester Syntax class expression. CURIEs resolve against
/// `pm` natively (`parse_class_expression` takes the `PrefixMapping`
/// directly), so no rewriting to full `<IRI>` form is needed here.
pub fn parse_ce(expr: &str, pm: &PrefixMapping) -> Result<ClassExpression<RcStr>, ClaimError> {
    let build: Build<RcStr> = Build::new();
    horned_owl::io::omn::reader::parse_class_expression(expr, pm, &build).map_err(|e| {
        ClaimError::ManchesterSyntax {
            expr: expr.to_string(),
            message: e.to_string(),
        }
    })
}

fn classify(t: &Triple) -> Result<Claim, ClaimError> {
    let subject = match &t.subject {
        NamedOrBlankNode::NamedNode(n) => n.as_str().to_string(),
        NamedOrBlankNode::BlankNode(b) => return Err(ClaimError::BlankNode(format!("{b}"))),
    };
    let predicate = t.predicate.as_str().to_string();

    match &t.object {
        Term::NamedNode(obj) => {
            let object = obj.as_str().to_string();
            Ok(match predicate.as_str() {
                RDF_TYPE => Claim::ClassAssertion {
                    individual: subject,
                    class: object,
                },
                RDFS_SUBCLASSOF if object == OWL_NOTHING => Claim::Unsatisfiable { class: subject },
                RDFS_SUBCLASSOF => Claim::Subsumption {
                    sub: subject,
                    sup: object,
                },
                OWL_EQUIVALENTCLASS => Claim::Equivalence {
                    left: subject,
                    right: object,
                },
                _ => Claim::ObjectPropertyAssertion {
                    subject,
                    property: predicate,
                    object,
                },
            })
        }
        Term::Literal(lit) => {
            if predicate == RDF_TYPE || predicate == RDFS_SUBCLASSOF {
                return Err(ClaimError::LiteralWithNonDataPredicate(predicate));
            }
            Ok(Claim::DataPropertyAssertion {
                subject,
                property: predicate,
                literal: Literal {
                    lexical: lit.value().to_string(),
                    datatype: lit.datatype().as_str().to_string(),
                    language: lit.language().map(str::to_string),
                },
            })
        }
        Term::BlankNode(b) => Err(ClaimError::BlankNode(format!("{b}"))),
    }
}
